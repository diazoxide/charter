//! D-90c (#1194): a persona's memory store and the shared store (`personas/_shared/memory`)
//! get the no-link hold a workspace's store has (V74). A link at or below the store, its
//! `archive/` included, is refused rather than followed, even one that stays inside the
//! project: no memory operation lists, reads, edits, archives or restores through it, and
//! nothing outside the store changes.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use purlis_core::memstore::Base;
use purlis_core::workspaces::Plane;

fn at() -> chrono::NaiveDateTime {
    "2026-10-09T09:00:00".parse().unwrap()
}

/// A project with the personas `devops` and `ops`, the shared store, and a workspace `beta`:
/// `ops` and `beta` each hold a memory and an archived one, which a link may point at.
fn project() -> tempfile::TempDir {
    let p = tempfile::tempdir().unwrap();
    let root = p.path();
    for persona in ["devops", "ops", "_shared"] {
        std::fs::create_dir_all(root.join("personas").join(persona)).unwrap();
        std::fs::write(
            root.join("personas").join(persona).join("persona.md"),
            "---\nrole: R\n---\n# R\n",
        )
        .unwrap();
    }
    for target in ["personas/ops/memory", "workspaces/beta/memory"] {
        let dir = root.join(target);
        std::fs::create_dir_all(dir.join("archive")).unwrap();
        std::fs::write(dir.join("MEMORY.md"), "# Theirs\n\n- [Theirs](theirs.md)\n").unwrap();
        std::fs::write(
            dir.join("theirs.md"),
            "# Theirs\n\n_2026-10-01 09:00 · persistent_\n\nTheirs.\n",
        )
        .unwrap();
        std::fs::write(dir.join("archive/gone.md"), "# Gone\n\nGone.\n").unwrap();
    }
    p
}

/// Every file and link under `dir`, with its bytes or its target.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = std::fs::symlink_metadata(&path).unwrap().file_type();
        if kind.is_symlink() {
            let target = std::fs::read_link(&path).unwrap();
            out.push((path, target.to_string_lossy().as_bytes().to_vec()));
        } else if kind.is_dir() {
            out.push((path.clone(), Vec::new()));
            out.extend(snapshot(&path));
        } else {
            out.push((path.clone(), std::fs::read(&path).unwrap()));
        }
    }
    out.sort();
    out
}

/// Everything a link could reach: the other persona and the workspace.
fn theirs(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut all = snapshot(&root.join("personas/ops"));
    all.extend(snapshot(&root.join("workspaces")));
    all
}

fn slugs(entries: &[purlis_core::workspaces::Entry]) -> Vec<String> {
    entries.iter().map(|one| one.slug.clone()).collect()
}

/// Every memory operation on `persona`'s store, with what each answered: the listings' slugs,
/// and whether each other operation answered Ok.
fn every_operation(root: &Path, persona: &str) -> Vec<(&'static str, String)> {
    let found = Plane::open(root).persona(persona).unwrap();
    let listed = |entries: std::io::Result<Vec<purlis_core::workspaces::Entry>>| match entries {
        Ok(entries) => slugs(&entries).join(","),
        Err(_) => String::new(),
    };
    vec![
        ("memories", listed(found.memories())),
        ("archived_memories", listed(found.archived_memories())),
        (
            "memory_count",
            purlis_core::personas::memory_count(root, persona).to_string(),
        ),
        (
            "open_memory",
            found.open_memory("theirs").is_ok().to_string(),
        ),
        (
            "edit_memory",
            found
                .edit_memory("theirs", "Mine now", "Mine.", Base::Overwrite)
                .is_ok()
                .to_string(),
        ),
        (
            "archive_memory",
            found.archive_memory("theirs").is_ok().to_string(),
        ),
        (
            "unarchive_memory",
            found.unarchive_memory("gone", None).is_ok().to_string(),
        ),
    ]
}

/// What a store that reaches nothing answers to every operation.
fn nothing() -> Vec<(&'static str, String)> {
    vec![
        ("memories", String::new()),
        ("archived_memories", String::new()),
        ("memory_count", "0".to_owned()),
        ("open_memory", "false".to_owned()),
        ("edit_memory", "false".to_owned()),
        ("archive_memory", "false".to_owned()),
        ("unarchive_memory", "false".to_owned()),
    ]
}

#[test]
fn a_persona_store_linked_to_another_persona_or_a_workspace_is_refused_and_nothing_changes() {
    purlis_core::unsteered!();
    for persona in ["devops", "_shared"] {
        for target in ["../ops/memory", "../../workspaces/beta/memory"] {
            let p = project();
            let root = p.path();
            std::os::unix::fs::symlink(target, root.join("personas").join(persona).join("memory"))
                .unwrap();
            let before = theirs(root);

            let answered = every_operation(root, persona);

            assert_eq!(answered, nothing(), "{persona} -> {target}");
            assert!(
                Plane::open(root)
                    .persona(persona)
                    .unwrap()
                    .remember_titled("Planted through a link", Some("Planted"), at())
                    .is_err(),
                "{persona} -> {target}: remember"
            );
            assert_eq!(theirs(root), before, "{persona} -> {target}");
        }
    }
}

#[test]
fn an_archive_linked_inside_the_project_is_neither_listed_nor_moved_through() {
    purlis_core::unsteered!();
    for persona in ["devops", "_shared"] {
        for target in [
            "../../ops/memory/archive",
            "../../../workspaces/beta/memory/archive",
        ] {
            let p = project();
            let root = p.path();
            let store = root.join("personas").join(persona).join("memory");
            std::fs::create_dir_all(&store).unwrap();
            std::fs::write(store.join("MEMORY.md"), "# Mine\n\n- [Mine](mine.md)\n").unwrap();
            std::fs::write(store.join("mine.md"), "# Mine\n\nMine.\n").unwrap();
            std::os::unix::fs::symlink(target, store.join("archive")).unwrap();
            let before = theirs(root);
            let found = Plane::open(root).persona(persona).unwrap();

            let listed = found.archived_memories();
            let archived = found.archive_memory("mine");
            let restored = found.unarchive_memory("gone", None);

            assert!(
                listed.as_ref().map_or(true, |found| found.is_empty()),
                "{persona} -> {target}: {listed:?}"
            );
            assert!(archived.is_err(), "{persona} -> {target}: {archived:?}");
            assert!(restored.is_err(), "{persona} -> {target}: {restored:?}");
            assert!(store.join("mine.md").is_file(), "{persona} -> {target}");
            assert_eq!(theirs(root), before, "{persona} -> {target}");
        }
    }
}

#[test]
fn one_archived_entry_linked_inside_the_project_is_neither_listed_nor_restored() {
    purlis_core::unsteered!();
    for persona in ["devops", "_shared"] {
        let p = project();
        let root = p.path();
        let archive = root
            .join("personas")
            .join(persona)
            .join("memory")
            .join("archive");
        std::fs::create_dir_all(&archive).unwrap();
        std::os::unix::fs::symlink(
            "../../../ops/memory/archive/gone.md",
            archive.join("gone.md"),
        )
        .unwrap();
        let before = theirs(root);
        let found = Plane::open(root).persona(persona).unwrap();

        let listed = found.archived_memories().unwrap();
        let restored = found.unarchive_memory("gone", None);

        assert!(listed.is_empty(), "{persona}: {listed:?}");
        assert!(restored.is_err(), "{persona}: {restored:?}");
        assert!(
            std::fs::symlink_metadata(archive.join("gone.md"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "{persona}"
        );
        assert!(
            !root
                .join("personas")
                .join(persona)
                .join("memory/gone.md")
                .exists(),
            "{persona}"
        );
        assert_eq!(theirs(root), before, "{persona}");
    }
}

#[test]
fn a_plain_persona_store_still_does_everything() {
    purlis_core::unsteered!();
    for persona in ["devops", "_shared"] {
        let p = project();
        let root = p.path();
        let found = Plane::open(root).persona(persona).unwrap();
        found.remember_titled("Kept", Some("Kept"), at()).unwrap();
        found
            .remember_titled("Retired", Some("Retired"), at())
            .unwrap();
        found
            .edit_memory("kept", "Kept still", "Kept.", Base::Overwrite)
            .unwrap();
        found.archive_memory("retired").unwrap();
        assert_eq!(slugs(&found.memories().unwrap()), ["kept"], "{persona}");
        assert_eq!(
            slugs(&found.archived_memories().unwrap()),
            ["retired"],
            "{persona}"
        );
        found.unarchive_memory("retired", None).unwrap();
        assert_eq!(
            slugs(&found.memories().unwrap()),
            ["kept", "retired"],
            "{persona}"
        );
        assert_eq!(purlis_core::personas::memory_count(root, persona), 2);
    }
}
