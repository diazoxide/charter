//! Moving a memory from one scope to another (KN-3, ADR 0065 Q13's follow-up): a workspace's
//! journal, a persona's memory and `_shared`. The one core operation the window's Move and
//! `charter workspace move-memory` / `charter persona move-memory` both call.
//!
//! The move is whole: the file is renamed, never copied, so no copy stays behind, and a move
//! that is refused leaves both stores exactly as they were.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use charter_core::memscope::{Scope, move_memory};
use charter_core::workspaces::Plane;

fn plane(tmp: &tempfile::TempDir) -> Plane {
    let root = tmp.path();
    std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    std::fs::create_dir_all(root.join("workspaces/beta")).unwrap();
    for persona in ["devops", "steward"] {
        std::fs::create_dir_all(root.join(format!("personas/{persona}/memory"))).unwrap();
        std::fs::write(
            root.join(format!("personas/{persona}/persona.md")),
            format!("# {persona}\n"),
        )
        .unwrap();
    }
    std::fs::create_dir_all(root.join("personas/_shared/memory")).unwrap();
    Plane::open(root)
}

fn at() -> chrono::NaiveDateTime {
    "2026-03-02T09:14:37".parse().unwrap()
}

fn later() -> chrono::NaiveDateTime {
    "2026-10-05T17:01:02".parse().unwrap()
}

fn ws(name: &str) -> Scope {
    Scope::Workspace(name.to_owned())
}

fn persona(name: &str) -> Scope {
    Scope::Persona(name.to_owned())
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn stem(path: &Path) -> String {
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

/// The index's link lines, without the header.
fn lines(index: &Path) -> Vec<String> {
    std::fs::read_to_string(index)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("- ["))
        .map(str::to_string)
        .collect()
}

/// Every file under `dir`, relative, with its text: what "nothing changed" is checked against.
fn tree(dir: &Path) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(at) = todo.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            let kind = std::fs::symlink_metadata(&path).unwrap();
            if kind.is_dir() {
                todo.push(path);
            } else if kind.is_file() {
                out.insert(
                    path.strip_prefix(dir).unwrap().to_path_buf(),
                    std::fs::read_to_string(&path).unwrap_or_default(),
                );
            } else {
                out.insert(
                    path.strip_prefix(dir).unwrap().to_path_buf(),
                    "<link>".into(),
                );
            }
        }
    }
    out
}

#[test]
fn a_workspace_memory_moved_to_a_persona_is_the_same_file_under_its_slug() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    let alpha = plane.workspace("alpha").unwrap();
    let path = alpha
        .remember("Deploys go through the canary\nAlways.", at())
        .unwrap();
    let before = read(&path);

    let moved = move_memory(
        &plane,
        &ws("alpha"),
        &stem(&path),
        &persona("devops"),
        later(),
    )
    .unwrap();

    assert_eq!(
        moved,
        tmp.path()
            .join("personas/devops/memory/deploys-go-through-the-canary.md"),
        "a persona's memory is named by its slug alone"
    );
    assert!(!path.exists(), "moved, not copied: nothing stays behind");
    assert_eq!(
        read(&moved),
        before,
        "the title and the stamp go with it, byte for byte"
    );
    assert_eq!(
        lines(&tmp.path().join("workspaces/alpha/memory/MEMORY.md")),
        Vec::<String>::new(),
        "the journal no longer lists it"
    );
    assert_eq!(
        lines(&tmp.path().join("personas/devops/memory/MEMORY.md")),
        ["- [Deploys go through the canary](deploys-go-through-the-canary.md)"]
    );
}

#[test]
fn a_persona_memory_moved_to_a_workspace_takes_its_own_stamp_as_the_prefix() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    let devops = plane.persona("devops").unwrap();
    let path = devops.remember("Rotate the keys monthly", at()).unwrap();
    let before = read(&path);

    let moved = move_memory(
        &plane,
        &persona("devops"),
        "rotate-the-keys-monthly",
        &ws("beta"),
        later(),
    )
    .unwrap();

    // The prefix is the memory's own stamp, not the time of the move: the journal lists by
    // name, and a moved memory sorts where it was recorded.
    assert_eq!(
        moved,
        tmp.path()
            .join("workspaces/beta/memory/20260302-091400-rotate-the-keys-monthly.md")
    );
    assert!(!path.exists());
    assert_eq!(read(&moved), before);
    assert_eq!(
        lines(&tmp.path().join("workspaces/beta/memory/MEMORY.md")),
        ["- [Rotate the keys monthly](20260302-091400-rotate-the-keys-monthly.md)"]
    );
    assert!(
        read(&tmp.path().join("workspaces/beta/memory/MEMORY.md"))
            .starts_with("# beta — task memory"),
        "a journal's index is made with the journal's own header"
    );
    assert_eq!(
        lines(&tmp.path().join("personas/devops/memory/MEMORY.md")),
        Vec::<String>::new()
    );
}

#[test]
fn a_memory_with_no_stamp_line_moved_to_a_workspace_is_prefixed_with_the_time_of_the_move() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    let hand = tmp.path().join("personas/devops/memory/by-hand.md");
    std::fs::write(&hand, "# By hand\n\nNo stamp here.\n").unwrap();

    let moved = move_memory(&plane, &persona("devops"), "by-hand", &ws("alpha"), later()).unwrap();

    assert_eq!(stem(&moved), "20261005-170102-by-hand");
    assert_eq!(read(&moved), "# By hand\n\nNo stamp here.\n");
}

#[test]
fn a_memory_moved_away_and_back_on_the_minute_has_its_first_name_again() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    let alpha = plane.workspace("alpha").unwrap();
    let path = alpha
        .remember("A fact", "2026-03-02T09:14:00".parse().unwrap())
        .unwrap();
    let name = stem(&path);

    let there = move_memory(&plane, &ws("alpha"), &name, &Scope::Shared, later()).unwrap();
    assert_eq!(there, tmp.path().join("personas/_shared/memory/a-fact.md"));
    let back = move_memory(&plane, &Scope::Shared, "a-fact", &ws("alpha"), later()).unwrap();

    assert_eq!(
        back, path,
        "its identity is its name, and it comes home under it"
    );
}

#[test]
fn a_journal_memory_moved_to_another_journal_keeps_its_whole_name() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    let path = plane
        .workspace("alpha")
        .unwrap()
        .remember("A fact", at())
        .unwrap();

    let moved = move_memory(&plane, &ws("alpha"), &stem(&path), &ws("beta"), later()).unwrap();

    assert_eq!(
        moved,
        tmp.path()
            .join("workspaces/beta/memory")
            .join(path.file_name().unwrap())
    );
}

#[test]
fn a_target_already_holding_a_memory_of_that_name_is_refused_and_nothing_moves() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    plane
        .persona("devops")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    plane
        .workspace("alpha")
        .unwrap()
        .remember("A fact", later())
        .unwrap();
    let before = tree(tmp.path());

    // Both ways: the persona holds `a-fact`, and the journal holds a memory whose name is
    // `a-fact` once its stamp is taken off.
    let to_persona = move_memory(
        &plane,
        &ws("alpha"),
        "20261005-170102-a-fact",
        &persona("devops"),
        later(),
    )
    .unwrap_err();
    let to_journal =
        move_memory(&plane, &persona("devops"), "a-fact", &ws("alpha"), later()).unwrap_err();

    assert_eq!(to_persona.kind(), ErrorKind::AlreadyExists, "{to_persona}");
    assert!(to_persona.to_string().contains("a-fact"), "{to_persona}");
    assert_eq!(to_journal.kind(), ErrorKind::AlreadyExists, "{to_journal}");
    assert_eq!(tree(tmp.path()), before);
}

#[test]
fn a_move_into_the_store_it_is_in_is_refused() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    plane
        .persona("devops")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    let before = tree(tmp.path());

    let refused = move_memory(
        &plane,
        &persona("devops"),
        "a-fact",
        &persona("devops"),
        later(),
    )
    .unwrap_err();

    assert_eq!(refused.kind(), ErrorKind::InvalidInput, "{refused}");
    assert_eq!(tree(tmp.path()), before);
}

#[test]
fn a_scope_the_project_does_not_have_is_refused_and_nothing_is_made() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    plane
        .persona("devops")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    let before = tree(tmp.path());

    let no_persona = move_memory(
        &plane,
        &persona("devops"),
        "a-fact",
        &persona("nobody"),
        later(),
    )
    .unwrap_err();
    let no_workspace =
        move_memory(&plane, &persona("devops"), "a-fact", &ws("gamma"), later()).unwrap_err();
    let bad_name =
        move_memory(&plane, &persona("devops"), "a-fact", &ws("../x"), later()).unwrap_err();

    assert_eq!(no_persona.kind(), ErrorKind::NotFound, "{no_persona}");
    assert!(no_persona.to_string().contains("nobody"), "{no_persona}");
    assert_eq!(no_workspace.kind(), ErrorKind::NotFound, "{no_workspace}");
    assert_eq!(bad_name.kind(), ErrorKind::InvalidInput, "{bad_name}");
    assert_eq!(tree(tmp.path()), before);
}

#[test]
fn a_memory_the_source_does_not_hold_is_not_found() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);

    let refused = move_memory(
        &plane,
        &persona("devops"),
        "nothing",
        &Scope::Shared,
        later(),
    )
    .unwrap_err();

    assert_eq!(refused.kind(), ErrorKind::NotFound, "{refused}");
}

#[cfg(unix)]
#[test]
fn a_target_store_that_is_a_link_out_of_the_project_is_refused() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    plane
        .persona("devops")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    std::fs::remove_dir(tmp.path().join("personas/steward/memory")).unwrap();
    std::os::unix::fs::symlink(outside.path(), tmp.path().join("personas/steward/memory")).unwrap();
    let before = tree(tmp.path());

    let refused = move_memory(
        &plane,
        &persona("devops"),
        "a-fact",
        &persona("steward"),
        later(),
    )
    .unwrap_err();

    assert_eq!(refused.kind(), ErrorKind::PermissionDenied, "{refused}");
    assert_eq!(tree(tmp.path()), before);
    assert!(
        tree(outside.path()).is_empty(),
        "nothing reached outside the project"
    );
}

#[cfg(unix)]
#[test]
fn a_target_store_the_filesystem_will_not_let_charter_write_is_refused_and_nothing_moves() {
    charter_core::unsteered!();
    use std::os::unix::fs::PermissionsExt as _;
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    plane
        .persona("devops")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    let locked = tmp.path().join("personas/steward/memory");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    // Root writes through a mode; there is nothing to prove then.
    if std::fs::write(locked.join("probe"), "").is_ok() {
        return;
    }
    let before = tree(tmp.path());

    let refused = move_memory(
        &plane,
        &persona("devops"),
        "a-fact",
        &persona("steward"),
        later(),
    )
    .unwrap_err();

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(refused.kind(), ErrorKind::PermissionDenied, "{refused}");
    assert_eq!(tree(tmp.path()), before);
}

#[test]
fn a_target_whose_index_is_not_a_file_is_refused_before_anything_moves() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    plane
        .persona("devops")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    std::fs::create_dir_all(tmp.path().join("personas/steward/memory/MEMORY.md")).unwrap();
    let before = tree(tmp.path());

    let refused = move_memory(
        &plane,
        &persona("devops"),
        "a-fact",
        &persona("steward"),
        later(),
    )
    .unwrap_err();

    assert_eq!(tree(tmp.path()), before, "{refused}");
    assert!(
        tmp.path()
            .join("personas/devops/memory/a-fact.md")
            .is_file()
    );
}

#[test]
fn a_journal_memory_moved_away_and_back_has_its_first_name_again_to_the_minute() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    // Recorded at 09:14:37: the name has the seconds, the stamp line has only the minute.
    let path = plane
        .workspace("alpha")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    assert_eq!(stem(&path), "20260302-091437-a-fact");
    let text = read(&path);

    move_memory(&plane, &ws("alpha"), &stem(&path), &Scope::Shared, later()).unwrap();
    let back = move_memory(&plane, &Scope::Shared, "a-fact", &ws("alpha"), later()).unwrap();

    // The guarantee is to the minute: the seconds come back as `00`, the text byte for byte.
    assert_eq!(stem(&back), "20260302-091400-a-fact");
    assert_eq!(read(&back), text);
}

#[cfg(unix)]
#[test]
fn a_persona_store_that_is_a_link_inside_the_project_is_refused_in_the_persona_stores_words() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    plane
        .persona("devops")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    std::fs::create_dir_all(tmp.path().join("workspaces/beta/memory")).unwrap();
    std::fs::remove_dir(tmp.path().join("personas/steward/memory")).unwrap();
    std::os::unix::fs::symlink(
        "../../workspaces/beta/memory",
        tmp.path().join("personas/steward/memory"),
    )
    .unwrap();
    let before = tree(tmp.path());

    let refused = move_memory(
        &plane,
        &persona("devops"),
        "a-fact",
        &persona("steward"),
        later(),
    )
    .unwrap_err();

    assert_eq!(refused.kind(), ErrorKind::PermissionDenied, "{refused}");
    let said = refused.to_string();
    assert!(said.contains("persona store"), "{said}");
    assert!(!said.contains("workspace"), "{said}");
    assert_eq!(tree(tmp.path()), before);
}

#[cfg(unix)]
#[test]
fn a_move_the_filesystem_refuses_makes_no_target_store() {
    charter_core::unsteered!();
    use std::os::unix::fs::PermissionsExt as _;
    let tmp = tempfile::tempdir().unwrap();
    let plane = plane(&tmp);
    plane
        .persona("devops")
        .unwrap()
        .remember("A fact", at())
        .unwrap();
    std::fs::remove_dir(tmp.path().join("personas/steward/memory")).unwrap();
    // The source store will not let the file leave it.
    let source = tmp.path().join("personas/devops/memory");
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(source.join("probe"), "").is_ok() {
        return;
    }

    let refused = move_memory(
        &plane,
        &persona("devops"),
        "a-fact",
        &persona("steward"),
        later(),
    )
    .unwrap_err();

    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(refused.kind(), ErrorKind::PermissionDenied, "{refused}");
    assert!(
        !tmp.path().join("personas/steward/memory").exists(),
        "a move that did not happen made no store"
    );
    assert!(source.join("a-fact.md").is_file());
}
