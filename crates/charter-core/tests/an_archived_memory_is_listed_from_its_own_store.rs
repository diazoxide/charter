//! A store's archive, listed for the window to browse and restore from (KN-4, D6): what
//! `archive/` holds, read with the same reader and the same gates as the store's own memories,
//! and nothing from any other store.

use charter_core::workspaces::Plane;
use std::fs;
use std::path::Path;

fn at() -> chrono::NaiveDateTime {
    "2026-05-04T11:32:17".parse().unwrap()
}

/// A plane with the workspace `alpha` and the persona `steward`.
fn plane() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    fs::create_dir_all(root.join("personas/steward/memory")).unwrap();
    fs::write(
        root.join("personas/steward/persona.md"),
        "---\nrole: Steward\n---\n# Steward\n",
    )
    .unwrap();
    tmp
}

fn slugs(entries: &[charter_core::workspaces::Entry]) -> Vec<&str> {
    entries.iter().map(|one| one.slug.as_str()).collect()
}

#[test]
fn a_persona_lists_what_its_archive_holds_and_not_what_its_store_holds() {
    charter_core::unsteered!();
    let tmp = plane();
    let steward = Plane::open(tmp.path()).persona("steward").unwrap();
    steward
        .remember_titled("eu-west-1", Some("Where prod-1 is"), at())
        .unwrap();
    steward
        .remember_titled("No deploys on Friday", Some("Freeze"), at())
        .unwrap();
    steward.archive_memory("freeze").unwrap();

    let archived = steward.archived_memories().unwrap();

    assert_eq!(slugs(&archived), ["freeze"]);
    assert_eq!(archived[0].title, "Freeze");
    assert_eq!(archived[0].stamp, "2026-05-04 11:32");
    assert_eq!(archived[0].body, "No deploys on Friday");
}

#[test]
fn a_workspace_lists_its_journals_archive() {
    charter_core::unsteered!();
    let tmp = plane();
    let alpha = Plane::open(tmp.path()).workspace("alpha").unwrap();
    let made = alpha
        .remember_titled("The API is slow on Mondays", None, at())
        .unwrap();
    let slug = made.file_stem().unwrap().to_string_lossy().into_owned();
    alpha.archive_memory(&slug).unwrap();

    let archived = alpha.archived_memories().unwrap();

    assert_eq!(slugs(&archived), [slug.as_str()]);
    assert_eq!(archived[0].body, "The API is slow on Mondays");
    assert!(alpha.memories().unwrap().is_empty());
}

#[test]
fn a_store_that_never_archived_anything_lists_none() {
    charter_core::unsteered!();
    let tmp = plane();
    let plane = Plane::open(tmp.path());

    assert!(
        plane
            .persona("steward")
            .unwrap()
            .archived_memories()
            .unwrap()
            .is_empty()
    );
    assert!(
        plane
            .workspace("alpha")
            .unwrap()
            .archived_memories()
            .unwrap()
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn a_journals_archive_a_chat_linked_elsewhere_lists_nothing_from_there() {
    charter_core::unsteered!();
    // V74: a workspace's store is held by descriptor, so a link planted as its `archive/` — to
    // a persona's memory, inside the plane — is not followed by the listing either.
    let tmp = plane();
    let root: &Path = tmp.path();
    let steward = Plane::open(root).persona("steward").unwrap();
    steward
        .remember_titled("Theirs", Some("Theirs"), at())
        .unwrap();
    fs::create_dir_all(root.join("workspaces/alpha/memory")).unwrap();
    std::os::unix::fs::symlink(
        "../../../personas/steward/memory",
        root.join("workspaces/alpha/memory/archive"),
    )
    .unwrap();

    let listed = Plane::open(root)
        .workspace("alpha")
        .unwrap()
        .archived_memories();

    assert!(
        listed.as_ref().map_or(true, |found| found.is_empty()),
        "{listed:?}"
    );
}
