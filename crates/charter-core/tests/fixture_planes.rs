//! Checks against the committed fixture planes, which the Python charter itself wrote (its
//! generator was retired with the Python oracle, ADR 0046, and the planes are data now). They
//! are the oracle that needs no Python to read: whatever charter put in these files is what a
//! Rust reader has to agree with.

use std::path::{Path, PathBuf};

fn planes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes")
}

fn manifests() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![planes()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("fixture planes are committed") {
            let path = entry.expect("readable fixture entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "workspace.json") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_fixture_manifest_is_owned_by_charter_because_our_digest_is_the_one_it_stored() {
    charter_core::unsteered!();
    let found = manifests();
    assert!(!found.is_empty(), "no workspace.json in the fixture planes");

    for path in found {
        let text = std::fs::read_to_string(&path).expect("readable manifest");
        let doc: serde_json::Value = serde_json::from_str(&text).expect("manifest is JSON");
        let stored = doc
            .get("charter_generated")
            .and_then(|v| v.as_str())
            .expect("charter writes charter_generated on every write");

        assert_eq!(
            charter_core::manifest::digest(&doc),
            stored,
            "digest mismatch for {}",
            path.display()
        );
        assert_eq!(
            charter_core::manifest::ownership(Some(&text)),
            charter_core::manifest::Ownership::Charter,
            "{} should read as charter's own",
            path.display()
        );
    }
}

#[test]
fn a_fixture_manifest_read_and_written_back_is_byte_identical() {
    charter_core::unsteered!();
    for path in manifests() {
        let text = std::fs::read_to_string(&path).expect("readable manifest");
        let doc: serde_json::Value = serde_json::from_str(&text).expect("manifest is JSON");

        assert_eq!(
            charter_core::pyjson::dumps_indent2(&doc),
            text,
            "round trip changed {}",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Reading a plane. Every expectation is a value in the `daily` fixture, which the
// Python charter wrote.

fn daily() -> charter_core::workspaces::Plane {
    charter_core::workspaces::Plane::open(planes().join("daily"))
}

#[test]
fn the_workspaces_of_a_plane_are_its_named_directories_in_order() {
    charter_core::unsteered!();
    assert_eq!(daily().workspaces().unwrap(), vec!["alpha", "beta"]);
}

#[test]
fn a_dotted_directory_under_workspaces_is_never_a_workspace() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    for name in [".worktrees", "zeta"] {
        std::fs::create_dir_all(dir.path().join("workspaces").join(name)).unwrap();
    }

    let plane = charter_core::workspaces::Plane::open(dir.path());

    assert_eq!(plane.workspaces().unwrap(), vec!["zeta"]);
}

#[test]
fn a_clone_that_landed_beside_the_workspaces_is_not_one_of_them() {
    charter_core::unsteered!();
    // `workspaces/<x>/.git` means somebody cloned into `workspaces/` itself.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(dir.path().join("workspaces/stray/.git")).unwrap();
    std::fs::create_dir_all(dir.path().join("workspaces/real")).unwrap();

    let plane = charter_core::workspaces::Plane::open(dir.path());

    assert_eq!(plane.workspaces().unwrap(), vec!["real"]);
}

#[test]
fn a_workspaces_vision_is_the_body_under_its_vision_heading() {
    charter_core::unsteered!();
    assert_eq!(
        daily().workspace("alpha").unwrap().vision(),
        "Ship the widget"
    );
    assert_eq!(
        daily().workspace("beta").unwrap().vision(),
        "Retire the old importer"
    );
}

#[test]
fn an_unset_vision_reads_as_empty_rather_than_as_its_placeholder() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    let ws = dir.path().join("workspaces/fresh");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(
        ws.join("workspace.md"),
        "# fresh\n\n## Vision\n\n_Not set yet — describe the goal._\n",
    )
    .unwrap();

    let plane = charter_core::workspaces::Plane::open(dir.path());

    assert_eq!(plane.workspace("fresh").unwrap().vision(), "");
}

#[test]
fn the_open_todos_of_a_workspace_are_the_files_its_index_lists() {
    charter_core::unsteered!();
    let todos = daily().workspace("alpha").unwrap().todos().unwrap();

    assert_eq!(todos.len(), 1, "one todo is open; the other was closed");
    assert_eq!(todos[0].title, "Review the rollout plan");
    assert_eq!(todos[0].slug, "20260302-091400-review-the-rollout-plan");
    assert_eq!(todos[0].body, "Review the rollout plan");
}

#[test]
fn a_workspace_that_was_never_given_a_todo_has_none_rather_than_failing() {
    charter_core::unsteered!();
    assert_eq!(daily().workspace("beta").unwrap().todos().unwrap(), vec![]);
}

#[test]
fn the_memories_of_a_workspace_come_back_in_the_order_their_names_give() {
    charter_core::unsteered!();
    let memories = daily().workspace("alpha").unwrap().memories().unwrap();

    assert_eq!(
        memories
            .iter()
            .map(|m| m.title.as_str())
            .collect::<Vec<_>>(),
        vec![
            "The API returns 418 on Mondays",
            "Closed todo: Write the migration"
        ]
    );
}

#[test]
fn the_personas_of_a_plane_are_the_directories_holding_a_persona_file() {
    charter_core::unsteered!();
    // `_shared` and `_dispatch` are charter's own, not personas.
    assert_eq!(daily().personas().unwrap(), vec!["devops", "steward"]);
}

#[test]
fn an_underscore_directory_is_not_a_persona_even_when_it_holds_a_persona_file() {
    charter_core::unsteered!();
    // `_shared` carries memory and refs for every persona and is not one itself. The
    // fixture planes do not put a `persona.md` in one, so only this says so.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    for name in ["_shared", "devops"] {
        let d = dir.path().join("personas").join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("persona.md"), "---\nname: x\n---\n").unwrap();
    }

    let plane = charter_core::workspaces::Plane::open(dir.path());

    assert_eq!(plane.personas().unwrap(), vec!["devops"]);
}

// ---------------------------------------------------------------------------
// Writing a plane. Expectations are what the Python charter produced for the same
// inputs, in a temp plane with the clock pinned.

fn temp_plane() -> (tempfile::TempDir, charter_core::workspaces::Plane) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(dir.path().join("workspaces/alpha")).unwrap();
    let plane = charter_core::workspaces::Plane::open(dir.path());
    (dir, plane)
}

fn pinned() -> chrono::NaiveDateTime {
    "2026-03-02T09:14:00".parse().unwrap()
}

#[test]
fn a_charter_is_scaffolded_from_the_template_with_the_vision_unset() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();

    ws.scaffold_charter().unwrap();

    let text = std::fs::read_to_string(ws.dir().join("workspace.md")).unwrap();
    assert!(
        text.starts_with("# alpha\n\n> **Living charter**"),
        "{text}"
    );
    assert!(
        text.contains("## Vision\n\n_Not set yet — describe the goal"),
        "{text}"
    );
    assert_eq!(ws.vision(), "", "a scaffolded charter has no vision yet");
}

#[test]
fn a_charter_that_already_exists_is_not_overwritten_by_scaffolding() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    std::fs::write(
        ws.dir().join("workspace.md"),
        "# mine\n\n## Vision\n\nkeep me\n",
    )
    .unwrap();

    ws.scaffold_charter().unwrap();

    assert_eq!(
        std::fs::read_to_string(ws.dir().join("workspace.md")).unwrap(),
        "# mine\n\n## Vision\n\nkeep me\n"
    );
}

#[test]
fn setting_a_vision_replaces_that_section_and_leaves_the_others_alone() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();

    ws.set_vision("Ship the widget").unwrap();

    let text = std::fs::read_to_string(ws.dir().join("workspace.md")).unwrap();
    assert_eq!(ws.vision(), "Ship the widget");
    assert!(
        text.contains("## Context & decisions"),
        "the template survives: {text}"
    );
    assert!(text.contains("## Glossary"));
    assert!(
        !text.contains("_Not set yet"),
        "the placeholder is gone: {text}"
    );
}

#[test]
fn a_todo_is_written_and_indexed_the_way_python_writes_one() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();

    ws.add_todo("Review the rollout plan", pinned()).unwrap();

    assert_eq!(
        std::fs::read_to_string(ws.dir().join("todos/MEMORY.md")).unwrap(),
        "# Todos — workspace `alpha`\n\nOne line per todo; each links a file holding one thing this task still means to do.\nOpen or done — and done removes it, leaving its trace in the journal instead.\n- [Review the rollout plan](20260302-091400-review-the-rollout-plan.md)\n"
    );
    let todos = ws.todos().unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].slug, "20260302-091400-review-the-rollout-plan");
}

#[test]
fn closing_a_todo_deletes_it_and_leaves_its_trace_in_the_journal() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    ws.add_todo("Write the migration", pinned()).unwrap();
    let slug = ws.todos().unwrap()[0].slug.clone();

    ws.close_todo(&slug, pinned()).unwrap();

    assert_eq!(ws.todos().unwrap(), vec![], "the todo file is gone");
    assert_eq!(
        std::fs::read_to_string(ws.dir().join("todos/MEMORY.md")).unwrap(),
        "# Todos — workspace `alpha`\n\nOne line per todo; each links a file holding one thing this task still means to do.\nOpen or done — and done removes it, leaving its trace in the journal instead.\n",
        "its index line is gone and the header stays"
    );
    let memories = ws.memories().unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].title, "Closed todo: Write the migration");
    assert_eq!(
        memories[0].slug,
        "20260302-091400-closed-todo-write-the-migration"
    );
}

#[test]
fn a_remembered_fact_lands_in_the_journal_with_its_index_header() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();

    ws.remember("The API returns 418 on Mondays", pinned())
        .unwrap();

    let index = std::fs::read_to_string(ws.dir().join("memory/MEMORY.md")).unwrap();
    assert!(
        index.starts_with("# alpha — task memory\n\nOne file per memory"),
        "{index}"
    );
    assert!(index.ends_with("- [The API returns 418 on Mondays](20260302-091200-the-api-returns-418-on-mondays.md)\n")
        || index.ends_with("- [The API returns 418 on Mondays](20260302-091400-the-api-returns-418-on-mondays.md)\n"), "{index}");
}

#[test]
fn a_written_manifest_carries_the_digest_of_what_it_now_says() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(
        r#"{"name":"alpha","description":"","repos":[],"updated_at":"2026-03-02T09:14:00+00:00","updated_by":"Fixture User","charter_generated":"stale"}"#,
    )
    .unwrap();
    doc["description"] = serde_json::Value::String("Ship the widget — fast".into());

    ws.write_manifest(&doc).unwrap();

    let text = std::fs::read_to_string(ws.dir().join("workspace.json")).unwrap();
    let back: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        charter_core::manifest::ownership(Some(&text)),
        charter_core::manifest::Ownership::Charter,
        "the stale digest was replaced: {text}"
    );
    assert_eq!(back["description"], "Ship the widget — fast");
    assert!(
        text.ends_with("}\n"),
        "indent=2 with one trailing newline: {text}"
    );
    assert!(
        text.contains("\"description\": \"Ship the widget \\u2014 fast\""),
        "non-ASCII is escaped on disk as Python escapes it: {text}"
    );
}

#[test]
fn two_writers_at_once_do_not_share_a_temp_file() {
    charter_core::unsteered!();
    // #893: `ensure` scaffolds a manifest and `record_members` rewrites one, and two
    // commands doing that at once for one workspace used to share a single
    // `workspace.json.tmp`. The pid in the temp name is what stops that, so this asks for
    // the property rather than the spelling.
    // `_keep`, not `_`: binding a value to `_` drops it at once, which deleted the plane
    // out from under the writers.
    let (_keep, plane) = temp_plane();
    let dir = plane.workspace("alpha").unwrap().dir().to_path_buf();

    std::thread::scope(|scope| {
        for n in 0..8 {
            let plane = plane.clone();
            scope.spawn(move || {
                let doc: serde_json::Value =
                    serde_json::from_str(&format!(r#"{{"name":"alpha","n":{n}}}"#)).unwrap();
                plane
                    .workspace("alpha")
                    .unwrap()
                    .write_manifest(&doc)
                    .expect("every writer succeeds");
            });
        }
    });

    let text = std::fs::read_to_string(dir.join("workspace.json")).unwrap();
    assert_eq!(
        charter_core::manifest::ownership(Some(&text)),
        charter_core::manifest::Ownership::Charter,
        "the survivor is one whole manifest, not a mix of two: {text}"
    );
    let strays: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert_eq!(strays, Vec::<String>::new(), "no temp file is left behind");
}

#[test]
fn writing_a_manifest_leaves_no_temp_file_behind() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    let doc: serde_json::Value = serde_json::from_str(r#"{"name":"alpha"}"#).unwrap();

    ws.write_manifest(&doc).unwrap();

    let strays: Vec<String> = std::fs::read_dir(ws.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n != "workspace.json")
        .collect();
    assert_eq!(
        strays,
        Vec::<String>::new(),
        "atomic write cleans up after itself"
    );
}

#[test]
fn a_todo_can_be_closed_by_its_bare_slug_without_the_timestamp() {
    charter_core::unsteered!();
    // `charter ws todo done write-the-migration` is how the fixture generator closes one.
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    ws.add_todo("Write the migration", pinned()).unwrap();

    ws.close_todo("write-the-migration", pinned()).unwrap();

    assert_eq!(ws.todos().unwrap(), vec![]);
    assert_eq!(
        ws.memories().unwrap()[0].title,
        "Closed todo: Write the migration"
    );
}

#[test]
fn the_planes_default_persona_is_the_one_charter_toml_names() {
    charter_core::unsteered!();
    assert_eq!(daily().default_persona(), Some("steward".to_string()));
}

#[test]
fn a_plane_whose_manifest_names_no_persona_has_no_default() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();

    assert_eq!(
        charter_core::workspaces::Plane::open(dir.path()).default_persona(),
        None
    );
}

#[test]
fn an_unparseable_charter_toml_names_no_persona_rather_than_failing() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = = =\n").unwrap();

    assert_eq!(
        charter_core::workspaces::Plane::open(dir.path()).default_persona(),
        None
    );
}

#[test]
fn a_path_inside_a_workspace_names_that_workspace() {
    charter_core::unsteered!();
    let plane = daily();
    let root = plane.root().to_path_buf();

    assert_eq!(
        plane.workspace_of(&root.join("workspaces/alpha")),
        Some("alpha".to_string())
    );
    assert_eq!(
        plane.workspace_of(&root.join("workspaces/alpha/svc/README.md")),
        Some("alpha".to_string()),
        "a chat working deep inside a clone still belongs to the workspace"
    );
}

#[test]
fn a_path_outside_the_workspaces_belongs_to_none_of_them() {
    charter_core::unsteered!();
    let plane = daily();
    let root = plane.root().to_path_buf();

    assert_eq!(plane.workspace_of(&root), None);
    assert_eq!(plane.workspace_of(&root.join("personas/devops")), None);
    assert_eq!(plane.workspace_of(Path::new("/tmp")), None);
}

#[test]
fn a_path_naming_a_workspace_that_is_not_there_belongs_to_none() {
    charter_core::unsteered!();
    let plane = daily();

    assert_eq!(
        plane.workspace_of(&plane.root().join("workspaces/ghost/x")),
        None
    );
}

// ---------------------------------------------------------------------------
// Containment, and the divergences an adversarial review of PR #21 found.

#[test]
fn a_name_that_walks_out_of_the_plane_is_not_a_workspace() {
    charter_core::unsteered!();
    // `Path::join` throws the prefix away for an absolute path and `..` walks out, so an
    // unchecked `-w` was a write anywhere on the filesystem.
    let (_tmp, plane) = temp_plane();

    for name in [
        "../../outside/escaped",
        "/tmp/absolute",
        "..",
        ".",
        "",
        "a/b",
    ] {
        assert!(
            plane.workspace(name).is_err(),
            "{name:?} must not name a workspace"
        );
        assert!(
            plane.persona(name).is_err(),
            "{name:?} must not name a persona either"
        );
    }
}

#[test]
fn a_linked_worktree_under_workspaces_is_still_a_workspace() {
    charter_core::unsteered!();
    // Git draws the line: a clone's `.git` is a directory, a worktree's is a file.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    let ws = dir.path().join("workspaces");
    std::fs::create_dir_all(ws.join("clone/.git")).unwrap();
    std::fs::create_dir_all(ws.join("worktree")).unwrap();
    std::fs::write(
        ws.join("worktree/.git"),
        "gitdir: /elsewhere/.git/worktrees/w\n",
    )
    .unwrap();

    let plane = charter_core::workspaces::Plane::open(dir.path());

    assert_eq!(plane.workspaces().unwrap(), vec!["worktree"]);
}

#[cfg(unix)]
#[test]
fn one_unreadable_memory_does_not_cost_the_whole_listing() {
    charter_core::unsteered!();
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    ws.remember("readable fact", pinned()).unwrap();
    let hidden = ws.dir().join("memory/unreadable.md");
    std::fs::write(&hidden, "# nope\n\n_2026-03-02 09:14 · persistent_\n\nx\n").unwrap();
    std::fs::set_permissions(&hidden, std::fs::Permissions::from_mode(0o000)).unwrap();

    let memories = ws.memories().unwrap();

    assert_eq!(
        memories
            .iter()
            .map(|m| m.title.as_str())
            .collect::<Vec<_>>(),
        vec!["readable fact"],
        "the unreadable one is skipped, not fatal"
    );
}

#[test]
fn a_memory_with_no_heading_is_named_by_its_file_stem() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    std::fs::create_dir_all(ws.dir().join("memory")).unwrap();
    std::fs::write(ws.dir().join("memory/hand-written.md"), "no heading here\n").unwrap();

    assert_eq!(ws.memories().unwrap()[0].title, "hand-written");
}

#[test]
fn a_title_that_itself_begins_with_a_hash_keeps_it() {
    charter_core::unsteered!();
    // charter takes `ln[2:].strip()`, once — not a repeated prefix strip.
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    std::fs::create_dir_all(ws.dir().join("memory")).unwrap();
    std::fs::write(
        ws.dir().join("memory/hashy.md"),
        "# # Hello\n\n_2026-03-02 09:14 · persistent_\n\nbody\n",
    )
    .unwrap();

    assert_eq!(ws.memories().unwrap()[0].title, "# Hello");
}

#[test]
fn a_body_of_only_separator_controls_is_empty_to_charter() {
    charter_core::unsteered!();
    // Python's `str.strip()` is driven by `str.isspace()`, true for U+001C–U+001F; Rust's
    // `White_Space` is false for all four. `write` refuses an empty body so a failed
    // substitution cannot land a secret in a memory file.
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();

    for body in ["\u{1c}", "\u{1d}", "\u{1e}", "\u{1f}", " \u{1f}\n"] {
        assert!(
            ws.remember(body, pinned()).is_err(),
            "{body:?} must read as an empty memory"
        );
        assert!(ws.add_todo(body, pinned()).is_err());
    }
}

#[test]
fn a_separator_control_is_stripped_from_a_title_and_a_body_as_python_strips_it() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();

    let path = ws.remember("\u{1f}Padded fact\u{1f}", pinned()).unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# Padded fact\n\n_2026-03-02 09:14 · persistent_\n\nPadded fact\n"
    );
}

#[test]
fn an_unparseable_manifest_belongs_to_the_operator_not_to_nobody() {
    charter_core::unsteered!();
    // This is the branch that stops the automatic writers putting a fresh manifest over the
    // hand-made file the rule exists to protect.
    use charter_core::manifest::{Ownership, ownership};

    assert_eq!(ownership(Some("{ not json")), Ownership::Operator);
    assert_eq!(ownership(Some("")), Ownership::Operator);
    assert_eq!(ownership(Some(r#"{"name":"alpha"}"#)), Ownership::Operator);
    assert_eq!(ownership(None), Ownership::Absent);
}

// ---------------------------------------------------------------------------
// Rules that a mutation survived, because nothing was asking about them.

#[test]
fn closing_a_todo_resolves_the_first_match_in_sorted_order() {
    charter_core::unsteered!();
    // `resolve` decides WHICH todo `ws todo done <slug>` closes. Two todos a second apart
    // share a bare slug, and charter takes the first in sorted order — the older one.
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    let early: chrono::NaiveDateTime = "2026-03-02T09:14:00".parse().unwrap();
    let later: chrono::NaiveDateTime = "2026-03-02T09:15:00".parse().unwrap();
    ws.add_todo("Write the migration", early).unwrap();
    ws.add_todo("Write the migration", later).unwrap();

    ws.close_todo("write-the-migration", later).unwrap();

    let left = ws.todos().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(
        left[0].slug, "20260302-091500-write-the-migration",
        "the OLDER one was closed; the first hit in sorted order wins"
    );
}

#[test]
fn a_bare_slug_matches_at_a_dash_and_nowhere_else() {
    charter_core::unsteered!();
    // The suffix is `-<ident>.md`. Verified against `charter.memstore.resolve` itself for
    // `20260302-091400-the-migration.md`:
    //   "migration"     -> found   (the dash before it is the boundary)
    //   "the-migration" -> found
    //   "gration"       -> None    (no dash in front of it)
    //   "e-migration"   -> None    (the dash is there, but not the whole segment)
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    ws.add_todo("the migration", pinned()).unwrap();
    let dir = ws.dir().join("todos");

    assert!(charter_core::memstore::resolve(plane.root(), &dir, "migration").is_some());
    assert!(charter_core::memstore::resolve(plane.root(), &dir, "the-migration").is_some());
    assert!(charter_core::memstore::resolve(plane.root(), &dir, "gration").is_none());
    assert!(charter_core::memstore::resolve(plane.root(), &dir, "e-migration").is_none());
}

#[test]
fn an_exact_filename_wins_over_a_suffix_match() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    let dir = ws.dir().join("todos");
    std::fs::create_dir_all(&dir).unwrap();
    // The direct name, and a timestamped file that also ends `-note.md`.
    for name in ["note.md", "20260302-091400-note.md"] {
        std::fs::write(
            dir.join(name),
            "# n\n\n_2026-03-02 09:14 · persistent_\n\nn\n",
        )
        .unwrap();
    }

    let found = charter_core::memstore::resolve(plane.root(), &dir, "note").unwrap();

    assert_eq!(found.file_name().unwrap(), "note.md");
}

#[test]
fn a_todo_is_still_open_if_its_closing_memory_could_not_be_written() {
    charter_core::unsteered!();
    // charter writes the journal entry FIRST and deletes second, so a failure leaves the
    // todo open rather than closed with nothing recorded.
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    ws.add_todo("Write the migration", pinned()).unwrap();
    // `memory` occupied by a FILE, so creating the store fails.
    std::fs::write(ws.dir().join("memory"), "in the way\n").unwrap();

    assert!(ws.close_todo("write-the-migration", pinned()).is_err());
    assert_eq!(
        ws.todos().unwrap().len(),
        1,
        "the todo survives a journal that could not be written"
    );
}

#[test]
fn an_explicit_title_is_capped_at_the_same_seventy_two_characters() {
    charter_core::unsteered!();
    assert_eq!(charter_core::memstore::TITLE_MAX, 72);
    assert_eq!(
        charter_core::memstore::title_of(&"z".repeat(100))
            .chars()
            .count(),
        72
    );
    assert_eq!(
        charter_core::memstore::title_of(&"z".repeat(72))
            .chars()
            .count(),
        72,
        "exactly 72 is not truncated"
    );
}

#[test]
fn every_break_python_splits_on_ends_a_line_here_too() {
    charter_core::unsteered!();
    use charter_core::mdsection::split_lines;

    for (breaker, name) in [
        ('\u{a}', "LF"),
        ('\u{b}', "VT"),
        ('\u{c}', "FF"),
        ('\u{d}', "CR"),
        ('\u{1c}', "FS"),
        ('\u{1d}', "GS"),
        ('\u{1e}', "RS"),
        ('\u{85}', "NEL"),
        ('\u{2028}', "LS"),
        ('\u{2029}', "PS"),
    ] {
        assert_eq!(
            split_lines(&format!("a{breaker}b")),
            vec!["a", "b"],
            "{name} must end a line"
        );
    }
}

#[test]
fn carriage_return_newline_is_one_break_and_not_two() {
    charter_core::unsteered!();
    assert_eq!(
        charter_core::mdsection::split_lines("a\r\nb"),
        vec!["a", "b"],
        "a CRLF file must not gain a blank line per row"
    );
}

#[test]
fn a_chat_in_a_directory_that_is_no_workspace_is_not_filed_under_an_invented_one() {
    charter_core::unsteered!();
    // Load-bearing for the sidebar: `plane_sidebar` puts a filed chat into `filed[name]`
    // and only ever drains keys that are real workspaces, so a chat filed under a name the
    // plane does not have vanishes from the window entirely — neither filed nor unfiled.
    //
    // The directory has to EXIST for this to test anything: for a path that is not there,
    // `canonicalize` fails and `strip_prefix` refuses it whatever the name check does. A
    // clone dropped straight into `workspaces/` is the real case — it is a directory, and
    // it is not a workspace.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    let stray = dir.path().join("workspaces/stray");
    std::fs::create_dir_all(stray.join(".git")).unwrap();
    std::fs::create_dir_all(stray.join("src")).unwrap();
    let plane = charter_core::workspaces::Plane::open(dir.path());
    assert_eq!(plane.workspaces().unwrap(), Vec::<String>::new());

    assert_eq!(plane.workspace_of(&stray.join("src")), None);
}

#[test]
fn the_refs_readme_is_not_a_memory() {
    charter_core::unsteered!();
    // `refs/README.md` sits beside a store and is not one of its entries.
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    ws.remember("a real fact", pinned()).unwrap();
    std::fs::write(ws.dir().join("memory/MEMORY.md.bak"), "not markdown\n").unwrap();

    let titles: Vec<String> = ws
        .memories()
        .unwrap()
        .into_iter()
        .map(|m| m.title)
        .collect();

    assert_eq!(titles, vec!["a real fact"]);
}

#[test]
fn an_index_emptied_of_its_last_entry_keeps_its_header() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    ws.add_todo("only one", pinned()).unwrap();

    charter_core::memstore::forget(plane.root(), &ws.dir().join("todos"), "only-one").unwrap();

    let index = std::fs::read_to_string(ws.dir().join("todos/MEMORY.md")).unwrap();
    assert!(
        index.starts_with("# Todos — workspace `alpha`\n"),
        "{index}"
    );
    assert!(
        index.ends_with('\n'),
        "one trailing newline survives: {index:?}"
    );
    assert!(!index.contains("- ["), "and no entry rows: {index:?}");
}

use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Personas and their memory.

#[test]
fn a_personas_memories_are_read_by_slug_with_no_timestamp_in_the_name() {
    charter_core::unsteered!();
    let devops = daily().persona("devops").unwrap();

    let memories = devops.memories().unwrap();

    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].slug, "cluster-prod-1-lives-in-eu-west-1");
    assert_eq!(memories[0].title, "Cluster prod-1 lives in eu-west-1");
    assert_eq!(memories[0].body, "Cluster prod-1 lives in eu-west-1");
}

#[test]
fn the_shared_store_is_the_one_every_persona_reads() {
    charter_core::unsteered!();
    let shared = daily().persona(charter_core::personas::SHARED).unwrap();

    let memories = shared.memories().unwrap();

    assert_eq!(
        memories
            .iter()
            .map(|m| m.title.as_str())
            .collect::<Vec<_>>(),
        vec!["The plane is the unit of work"]
    );
}

#[test]
fn a_persona_carries_the_role_its_charter_file_declares() {
    charter_core::unsteered!();
    assert_eq!(
        daily().persona("devops").unwrap().role(),
        Some("DevOps Engineer".to_string())
    );
    assert_eq!(daily().persona("nobody").unwrap().role(), None);
}

#[test]
fn scaffolding_a_persona_writes_the_index_and_the_refs_readme_charter_writes() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let devops = plane.persona("devops").unwrap();

    devops.scaffold_memory().unwrap();

    assert_eq!(
        std::fs::read_to_string(devops.dir().join("memory/MEMORY.md")).unwrap(),
        "# Memory Index — devops\n\nOne line per memory; each links a file holding a single durable fact.\nWritten by the persona as it learns; committed and shared.\n"
    );
    assert_eq!(
        std::fs::read_to_string(devops.dir().join("refs/README.md")).unwrap(),
        "# References — devops\n\nCurated docs, links, and snippets this role collects. Committed and shared. Never store secrets here — those live only in the vault.\n"
    );
}

#[test]
fn the_shared_store_is_scaffolded_for_all_personas_not_for_one_named_shared() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let shared = plane.persona(charter_core::personas::SHARED).unwrap();

    shared.scaffold_memory().unwrap();

    assert!(
        std::fs::read_to_string(shared.dir().join("memory/MEMORY.md"))
            .unwrap()
            .starts_with("# Memory Index — shared (all personas)\n"),
    );
}

#[test]
fn a_remembered_persona_fact_is_named_for_its_slug_alone() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let devops = plane.persona("devops").unwrap();
    devops.scaffold_memory().unwrap();

    let path = devops
        .remember("Cluster prod-1 lives in eu-west-1", pinned())
        .unwrap();

    assert_eq!(
        path.file_name().unwrap(),
        "cluster-prod-1-lives-in-eu-west-1.md"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# Cluster prod-1 lives in eu-west-1\n\n_2026-03-02 09:14 · persistent_\n\nCluster prod-1 lives in eu-west-1\n"
    );
    assert_eq!(
        std::fs::read_to_string(devops.dir().join("memory/MEMORY.md")).unwrap(),
        "# Memory Index — devops\n\nOne line per memory; each links a file holding a single durable fact.\nWritten by the persona as it learns; committed and shared.\n- [Cluster prod-1 lives in eu-west-1](cluster-prod-1-lives-in-eu-west-1.md)\n"
    );
}

// ---------------------------------------------------------------------------
// Which harness profiles the operator approved. Every unreadable state means ASK AGAIN.

use charter_core::profiletrust::{self, Fingerprint};

fn a_print() -> Fingerprint {
    Fingerprint {
        kind: "claude".into(),
        command: vec!["claude".into(), "--dangerously-skip-permissions".into()],
        env: [("CLAUDE_CONFIG_DIR".to_string(), "~/.work".to_string())]
            .into_iter()
            .collect(),
    }
}

#[test]
fn a_plane_with_no_record_has_approved_nothing() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();

    assert_eq!(
        profiletrust::last_launched(plane.root(), "claude-work"),
        None
    );
}

#[test]
fn a_record_charter_cannot_parse_reads_as_no_approval_rather_than_as_one() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let dir = plane.root().join(".charter");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(profiletrust::RECORD), "{ this is not json").unwrap();

    assert_eq!(
        profiletrust::last_launched(plane.root(), "claude-work"),
        None,
        "a file charter cannot read says nothing about what was approved"
    );
}

#[test]
fn an_entry_that_is_not_a_fingerprint_is_not_an_approval() {
    charter_core::unsteered!();
    // The file is a plain JSON object a chat can write, so this has to read as no record at
    // all rather than as something to compare against.
    let (_tmp, plane) = temp_plane();
    let dir = plane.root().join(".charter");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(profiletrust::RECORD),
        "{\n  \"claude-work\": \"approved, honest\"\n}\n",
    )
    .unwrap();

    assert_eq!(
        profiletrust::last_launched(plane.root(), "claude-work"),
        None
    );
}

#[test]
fn recording_a_launch_writes_the_fingerprint_as_declared() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();

    profiletrust::record_launched(plane.root(), "claude-work", &a_print()).unwrap();

    assert_eq!(
        std::fs::read_to_string(plane.root().join(".charter").join(profiletrust::RECORD)).unwrap(),
        "{\n  \"claude-work\": {\n    \"kind\": \"claude\",\n    \"command\": [\n      \"claude\",\n      \"--dangerously-skip-permissions\"\n    ],\n    \"env\": {\n      \"CLAUDE_CONFIG_DIR\": \"~/.work\"\n    }\n  }\n}\n",
        "`~` is recorded unexpanded: the file is what an edit changes"
    );
    assert_eq!(
        profiletrust::last_launched(plane.root(), "claude-work"),
        Some(a_print())
    );
}

#[test]
fn approving_one_profile_does_not_make_another_ask_again() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    profiletrust::record_launched(plane.root(), "claude-work", &a_print()).unwrap();

    let other = Fingerprint {
        kind: "codex".into(),
        command: vec!["codex".into()],
        env: BTreeMap::new(),
    };
    profiletrust::record_launched(plane.root(), "codex-work", &other).unwrap();

    assert_eq!(
        profiletrust::last_launched(plane.root(), "claude-work"),
        Some(a_print())
    );
    assert_eq!(
        profiletrust::last_launched(plane.root(), "codex-work"),
        Some(other)
    );
}

#[test]
fn re_recording_a_profile_keeps_the_place_it_already_had() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    profiletrust::record_launched(plane.root(), "aaa", &a_print()).unwrap();
    profiletrust::record_launched(plane.root(), "zzz", &a_print()).unwrap();

    let changed = Fingerprint {
        kind: "claude".into(),
        command: vec!["claude".into()],
        env: BTreeMap::new(),
    };
    profiletrust::record_launched(plane.root(), "aaa", &changed).unwrap();

    let text =
        std::fs::read_to_string(plane.root().join(".charter").join(profiletrust::RECORD)).unwrap();
    assert!(
        text.find("\"aaa\"") < text.find("\"zzz\""),
        "an updated key keeps its position: {text}"
    );
}

#[cfg(unix)]
#[test]
fn the_record_and_its_directory_are_private_to_the_operator() {
    charter_core::unsteered!();
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, plane) = temp_plane();

    profiletrust::record_launched(plane.root(), "claude-work", &a_print()).unwrap();

    let dir = plane.root().join(".charter");
    let file = dir.join(profiletrust::RECORD);
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600,
        "it records consent to run a command"
    );
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn a_non_ascii_env_value_is_escaped_in_the_record_as_python_escapes_it() {
    charter_core::unsteered!();
    // Verified against `charter.profiletrust.record_launched` itself: this file is
    // `json.dumps(..., indent=2)`, so `ensure_ascii` applies here as it does to a manifest.
    let (_tmp, plane) = temp_plane();
    let print = Fingerprint {
        kind: "claude".into(),
        command: vec!["claude".into()],
        env: [("LANG_HINT".to_string(), "café — ok".to_string())]
            .into_iter()
            .collect(),
    };

    profiletrust::record_launched(plane.root(), "fancy", &print).unwrap();

    let text =
        std::fs::read_to_string(plane.root().join(".charter").join(profiletrust::RECORD)).unwrap();
    assert!(
        text.contains(r#""LANG_HINT": "caf\u00e9 \u2014 ok""#),
        "{text}"
    );
    assert_eq!(
        profiletrust::last_launched(plane.root(), "fancy"),
        Some(print),
        "and it reads back as what was written"
    );
}

#[test]
fn a_workspace_symlinked_out_of_the_plane_cannot_be_written_through() {
    charter_core::unsteered!();
    // The name rule cannot see this: `escape` is a perfectly legal name. What redirects the
    // write is a COMMITTED symlink, which travels with the plane to every machine that
    // clones it. Python refuses it in `contain.writable` with the same reasoning.
    let (_tmp, plane) = temp_plane();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("workspace.md"), "# untouched\n").unwrap();
    std::os::unix::fs::symlink(
        outside.path(),
        plane.root().join("workspaces").join("escape"),
    )
    .unwrap();
    let ws = plane.workspace("escape").expect("the NAME is legal");

    assert!(ws.set_vision("written through a symlink").is_err());
    assert!(ws.remember("a fact", pinned()).is_err());
    assert!(ws.add_todo("a todo", pinned()).is_err());
    assert!(ws.scaffold_charter().is_err());
    assert!(
        ws.write_manifest(&serde_json::from_str(r#"{"name":"escape"}"#).unwrap())
            .is_err()
    );

    assert_eq!(
        std::fs::read_to_string(outside.path().join("workspace.md")).unwrap(),
        "# untouched\n",
        "nothing outside the plane was written"
    );
}

#[test]
fn a_persona_symlinked_out_of_the_plane_cannot_be_written_through() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(plane.root().join("personas")).unwrap();
    std::os::unix::fs::symlink(outside.path(), plane.root().join("personas").join("devops"))
        .unwrap();
    let persona = plane.persona("devops").expect("the NAME is legal");

    assert!(persona.scaffold_memory().is_err());
    assert!(persona.remember("a fact", pinned()).is_err());
    assert_eq!(
        std::fs::read_dir(outside.path()).unwrap().count(),
        0,
        "nothing outside the plane was written"
    );
}

// ---------------------------------------------------------------------------
// Containment, one path component deeper. A second review reproduced every one of these
// through the real CLI at exit 0; each is the same class as the workspace-directory link,
// which is why checking only that directory was not enough.

/// A plane with `workspaces/alpha`, and a directory outside it holding `outside.txt`.
fn plane_and_outside() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    charter_core::workspaces::Plane,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(dir.path().join("workspaces/alpha")).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("outside.txt"), "PRIVATE\n").unwrap();
    let plane = charter_core::workspaces::Plane::open(dir.path());
    (dir, outside, plane)
}

#[test]
fn a_charter_file_that_is_a_link_out_of_the_plane_is_not_written_through() {
    charter_core::unsteered!();
    let (_plane_dir, outside, plane) = plane_and_outside();
    let victim = outside.path().join("victim");
    std::fs::write(&victim, "original\n").unwrap();
    let ws = plane.workspace("alpha").unwrap();
    std::os::unix::fs::symlink(&victim, ws.dir().join("workspace.md")).unwrap();

    assert!(ws.set_vision("PWNED").is_err());
    assert!(ws.scaffold_charter().is_err());

    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "original\n");
}

#[test]
fn a_memory_store_that_is_a_link_out_of_the_plane_is_not_written_into() {
    charter_core::unsteered!();
    let (_plane_dir, outside, plane) = plane_and_outside();
    let ws = plane.workspace("alpha").unwrap();
    std::os::unix::fs::symlink(outside.path(), ws.dir().join("memory")).unwrap();

    assert!(ws.remember("a fact", pinned()).is_err());

    assert_eq!(
        std::fs::read_dir(outside.path()).unwrap().count(),
        1,
        "only the file the fixture put there"
    );
}

#[test]
fn a_manifest_that_is_a_link_out_of_the_plane_is_not_written_through() {
    charter_core::unsteered!();
    let (_plane_dir, outside, plane) = plane_and_outside();
    let victim = outside.path().join("victim.json");
    std::fs::write(&victim, "{}\n").unwrap();
    let ws = plane.workspace("alpha").unwrap();
    std::os::unix::fs::symlink(&victim, ws.dir().join("workspace.json")).unwrap();

    assert!(
        ws.write_manifest(&serde_json::from_str(r#"{"name":"alpha"}"#).unwrap())
            .is_err()
    );

    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "{}\n");
}

#[test]
fn a_vision_behind_a_link_out_of_the_plane_is_not_printed() {
    charter_core::unsteered!();
    // charter #442: a committed `workspaces/evil -> ../../elsewhere` with a LEGAL name
    // printed a file from outside the plane. Containing the name does not contain this.
    let (plane_dir, outside, plane) = plane_and_outside();
    std::fs::write(
        outside.path().join("workspace.md"),
        "# evil\n\n## Vision\n\nEXFILTRATED\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.path(),
        plane_dir.path().join("workspaces").join("evil"),
    )
    .unwrap();

    let ws = plane.workspace("evil").expect("the NAME is legal");

    assert_eq!(ws.vision(), "", "the outside file is not read");
    assert_eq!(ws.manifest().1, charter_core::manifest::Ownership::Absent);
}

#[test]
fn a_store_behind_a_link_out_of_the_plane_is_not_listed() {
    charter_core::unsteered!();
    let (_plane_dir, outside, plane) = plane_and_outside();
    std::fs::write(
        outside.path().join("secret.md"),
        "# Board minutes\n\n_2026-03-02 09:14 · persistent_\n\nCONFIDENTIAL\n",
    )
    .unwrap();
    let ws = plane.workspace("alpha").unwrap();
    std::os::unix::fs::symlink(outside.path(), ws.dir().join("todos")).unwrap();

    assert!(ws.todos().is_err(), "the store itself resolves outside");
}

#[test]
fn a_single_entry_that_links_out_of_the_plane_is_left_out_of_the_listing() {
    charter_core::unsteered!();
    // The store is legitimate; one file in it is a link out. Python gates each entry, not
    // only the directory.
    let (_plane_dir, outside, plane) = plane_and_outside();
    let ws = plane.workspace("alpha").unwrap();
    ws.remember("a real fact", pinned()).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("outside.txt"),
        ws.dir().join("memory/leak.md"),
    )
    .unwrap();

    let titles: Vec<String> = ws
        .memories()
        .unwrap()
        .into_iter()
        .map(|m| m.title)
        .collect();

    assert_eq!(titles, vec!["a real fact"]);
}

#[test]
fn a_persona_charter_behind_a_link_out_of_the_plane_is_not_read() {
    charter_core::unsteered!();
    let (plane_dir, outside, plane) = plane_and_outside();
    std::fs::write(
        outside.path().join("persona.md"),
        "---\nname: evil\nrole: Exfiltrator\n---\n",
    )
    .unwrap();
    std::fs::create_dir_all(plane_dir.path().join("personas")).unwrap();
    std::os::unix::fs::symlink(
        outside.path(),
        plane_dir.path().join("personas").join("devops"),
    )
    .unwrap();

    let persona = plane.persona("devops").expect("the NAME is legal");

    assert_eq!(persona.role(), None);
    assert!(persona.memories().is_err());
}

#[test]
fn the_consent_record_is_not_written_through_a_link_that_leaves_the_plane() {
    charter_core::unsteered!();
    // This file says which commands the operator approved RUNNING.
    let (plane_dir, outside, _plane) = plane_and_outside();
    std::os::unix::fs::symlink(outside.path(), plane_dir.path().join(".charter")).unwrap();

    let print = Fingerprint {
        kind: "claude".into(),
        command: vec!["claude".into()],
        env: BTreeMap::new(),
    };
    assert!(profiletrust::record_launched(plane_dir.path(), "x", &print).is_err());

    assert!(
        !outside.path().join(profiletrust::RECORD).exists(),
        "no consent record outside the plane"
    );
}

#[test]
fn only_the_planes_own_data_directories_are_writable() {
    charter_core::unsteered!();
    // Python's data roots are `personas/`, `workspaces/` and `.charter/persona-state` —
    // NOT `.charter` wholesale, which would put the vaults inside the allowlist.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    let root = dir.path();

    for ok in [
        "workspaces/alpha/workspace.md",
        "personas/devops/persona.md",
        ".charter/persona-state/ephemeral/s/x.md",
    ] {
        assert_eq!(
            charter_core::contain::writable(root, &root.join(ok)),
            Ok(()),
            "{ok} is data"
        );
    }
    for refused in [
        "docs/topology.md",
        ".charter/vaults/fixture.json",
        "charter.toml",
    ] {
        assert!(
            charter_core::contain::writable(root, &root.join(refused)).is_err(),
            "{refused} is not a data directory"
        );
    }
}

#[test]
fn a_record_missing_a_field_is_no_record_at_all() {
    charter_core::unsteered!();
    // The consent bypass this guards: if a missing `env` defaulted to empty instead of
    // failing, `{"kind":"claude","command":["claude"]}` would reconstruct as a fingerprint
    // EQUAL to a profile declared with no env — and launch without asking.
    let (_tmp, plane) = temp_plane();
    let dir = plane.root().join(".charter");
    std::fs::create_dir_all(&dir).unwrap();

    for doc in [
        r#"{"p": {"kind":"claude","command":["claude"]}}"#,
        r#"{"p": {"kind":"claude","env":{}}}"#,
        r#"{"p": {"command":["claude"],"env":{}}}"#,
        r#"{"p": {"kind":"claude","command":"claude","env":{}}}"#,
        r#"{"p": {"kind":"claude","command":["claude"],"env":{"A":1}}}"#,
    ] {
        std::fs::write(dir.join(profiletrust::RECORD), doc).unwrap();
        assert_eq!(
            profiletrust::last_launched(plane.root(), "p"),
            None,
            "{doc} must read as no record, so charter asks again"
        );
    }
}

#[test]
fn an_index_is_not_created_through_a_dangling_symlink() {
    charter_core::unsteered!();
    // `exists()` is false for a dangling link, which HELPS an attacker: a plain write would
    // create whatever the link names. The create is exclusive so the kernel refuses it.
    let (_tmp, plane) = temp_plane();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("planted");
    let ws = plane.workspace("alpha").unwrap();
    let store = ws.dir().join("memory");
    std::fs::create_dir_all(&store).unwrap();
    std::os::unix::fs::symlink(&target, store.join("MEMORY.md")).unwrap();

    let _ = charter_core::memstore::ensure_index(plane.root(), &store, "# Memory Index\n\n");

    assert!(
        !target.exists(),
        "the file the link named must not have been created"
    );
}

#[test]
fn a_persona_index_is_not_created_through_a_dangling_symlink() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("planted");
    std::fs::create_dir_all(plane.root().join("personas/devops/memory")).unwrap();
    std::os::unix::fs::symlink(
        &target,
        plane.root().join("personas/devops/memory/MEMORY.md"),
    )
    .unwrap();

    let _ = plane.persona("devops").unwrap().scaffold_memory();

    assert!(!target.exists());
}

// ---------------------------------------------------------------------------
// A third review walked through the containment fix. These are what it found.

#[test]
fn a_slug_that_walks_out_of_the_store_deletes_nothing() {
    charter_core::unsteered!();
    // charter #339: `memstore.resolve` applies no containment of its own, so
    // `../../<other>/todos/<slug>` resolved to a NEIGHBOUR's file and `unlink` took it.
    // charter gates the slug with `segment_ok` before it ever reaches the store.
    let (_plane_dir, outside, plane) = plane_and_outside();
    let victim = outside.path().join("victim.md");
    std::fs::write(&victim, "PRECIOUS\n").unwrap();
    let ws = plane.workspace("alpha").unwrap();
    let store = ws.dir().join("todos");
    std::fs::create_dir_all(&store).unwrap();

    for slug in [
        "../../../../victim",
        "../beta/todos/victim",
        "/tmp/victim",
        "..",
        // A legal FILENAME that is not a legal slug — the first spelling of the rule let
        // this through because it ended `.md`.
        "../../../../victim.md",
    ] {
        let refused = charter_core::memstore::forget(plane.root(), &store, slug)
            .expect_err("a traversing slug is refused");
        // The SLUG rule specifically, not merely "something said no": the containment gate
        // would also stop this, and a test that accepts either pins neither.
        assert!(
            refused.to_string().contains("is not the slug of one todo"),
            "{slug:?} must be refused as a slug, got: {refused}"
        );
    }

    assert!(victim.exists(), "nothing outside the store was deleted");
}

#[test]
fn closing_a_todo_by_a_traversing_slug_deletes_nothing() {
    charter_core::unsteered!();
    let (plane_dir, _outside, plane) = plane_and_outside();
    std::fs::create_dir_all(plane_dir.path().join("workspaces/beta/todos")).unwrap();
    let neighbour = plane_dir.path().join("workspaces/beta/todos/victim.md");
    std::fs::write(
        &neighbour,
        "# Victim\n\n_2026-03-02 09:14 · persistent_\n\nx\n",
    )
    .unwrap();
    let ws = plane.workspace("alpha").unwrap();
    ws.add_todo("Decoy", pinned()).unwrap();

    assert!(ws.close_todo("../../beta/todos/victim", pinned()).is_err());

    assert!(
        neighbour.exists(),
        "a neighbour's todo is not ours to close"
    );
}

#[test]
fn a_memory_index_that_links_out_of_the_plane_is_not_appended_to() {
    charter_core::unsteered!();
    // The store directory is legitimate; `MEMORY.md` inside it is a committed link out.
    let (_plane_dir, outside, plane) = plane_and_outside();
    let kept = outside.path().join("important");
    std::fs::write(&kept, "PRECIOUS OPERATOR DATA\n").unwrap();
    let ws = plane.workspace("alpha").unwrap();
    let store = ws.dir().join("memory");
    std::fs::create_dir_all(&store).unwrap();
    std::os::unix::fs::symlink(&kept, store.join("MEMORY.md")).unwrap();

    assert!(ws.remember("A durable fact", pinned()).is_err());

    assert_eq!(
        std::fs::read_to_string(&kept).unwrap(),
        "PRECIOUS OPERATOR DATA\n"
    );
}

#[test]
fn a_dangling_index_link_is_judged_by_where_it_points() {
    charter_core::unsteered!();
    // `canonicalize` fails for a dangling link, and judging it by its own name let it pass —
    // after which the write CREATED the file it named.
    let (_plane_dir, outside, plane) = plane_and_outside();
    let planted = outside.path().join("planted");
    let ws = plane.workspace("alpha").unwrap();
    let store = ws.dir().join("memory");
    std::fs::create_dir_all(&store).unwrap();
    std::os::unix::fs::symlink(&planted, store.join("MEMORY.md")).unwrap();

    assert!(ws.remember("A durable fact", pinned()).is_err());

    assert!(!planted.exists(), "the link's target was not created");
}

#[test]
fn a_path_whose_parent_walks_out_with_dot_dot_is_refused() {
    charter_core::unsteered!();
    // `Path::file_name()` is `None` for `..`, so an ancestor walk that collects names
    // dropped them: the reconstructed path was inside and the write landed in `/etc`.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    let root = dir.path();

    for escape in [
        "workspaces/alpha/missing/../../../../../../../etc/newfile",
        "workspaces/../../../etc/newfile",
        "workspaces/alpha/../../..",
    ] {
        assert!(
            charter_core::contain::writable(root, &root.join(escape)).is_err(),
            "{escape} leaves the plane"
        );
    }
    // A `..` that stays inside is still fine.
    assert_eq!(
        charter_core::contain::writable(root, &root.join("workspaces/beta/../alpha/x.md")),
        Ok(())
    );
}

#[test]
fn a_duplicate_check_does_not_read_a_store_outside_the_plane() {
    charter_core::unsteered!();
    // It reported the heading of an outside file on stderr — charter #442's shape, on the
    // one read path the gate had missed.
    let (_plane_dir, outside, plane) = plane_and_outside();
    std::fs::write(
        outside.path().join("leak.md"),
        "# Leaked heading from outside\n\n_2026-03-02 09:14 · persistent_\n\nsecret\n",
    )
    .unwrap();
    let ws = plane.workspace("alpha").unwrap();
    std::os::unix::fs::symlink(outside.path(), ws.dir().join("todos")).unwrap();

    assert_eq!(
        charter_core::memstore::duplicate_of(
            plane.root(),
            &ws.dir().join("todos"),
            "Leaked heading from outside"
        ),
        None
    );
}

#[test]
fn a_heading_below_the_first_line_is_still_the_title() {
    charter_core::unsteered!();
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha").unwrap();
    let store = ws.dir().join("todos");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(
        store.join("20260101-000000-t.md"),
        "\n\n# The real title\n\n_2026-01-01 00:00 · persistent_\n\nbody\n",
    )
    .unwrap();

    assert_eq!(ws.todos().unwrap()[0].title, "The real title");
}

#[test]
fn a_persona_index_that_resolves_out_of_the_plane_is_refused_by_containment() {
    charter_core::unsteered!();
    // `create_absent` gates the directory AND the file it opens. Only the directory gate is
    // needed to keep the file from being written — `create_new` refuses a link at the name
    // — so this asserts WHICH refusal fires. Without the file gate the error is the
    // filesystem's `AlreadyExists`, which is being stopped by a flag rather than by
    // containment, and that is the shape that produced five rounds of findings.
    let (plane_dir, outside, plane) = plane_and_outside();
    let store = plane_dir.path().join("personas/devops/memory");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(outside.path().join("index"), b"OUTSIDE\n").unwrap();
    std::os::unix::fs::symlink(outside.path().join("index"), store.join("MEMORY.md")).unwrap();

    let refused = plane
        .persona("devops")
        .unwrap()
        .scaffold_memory()
        .expect_err("an index that resolves outside the plane is refused");

    assert_eq!(
        refused.kind(),
        std::io::ErrorKind::PermissionDenied,
        "refused by containment, not by O_EXCL: {refused}"
    );
    assert!(
        refused.to_string().contains("outside the directories"),
        "{refused}"
    );
    assert_eq!(
        std::fs::read_to_string(outside.path().join("index")).unwrap(),
        "OUTSIDE\n"
    );
}

#[test]
fn a_consent_record_that_resolves_out_of_the_plane_is_refused_by_containment() {
    charter_core::unsteered!();
    // Same shape: `private_dir` refusing a symlinked `.charter` and the write ending in a
    // `rename` both keep the bytes inside, so this asserts that the REFUSAL is containment's
    // rather than a side effect of how the write happens to be done.
    let (plane_dir, outside, _plane) = plane_and_outside();
    let state = plane_dir.path().join(".charter");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(outside.path().join("record.json"), b"{}\n").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("record.json"),
        state.join(profiletrust::RECORD),
    )
    .unwrap();

    let print = Fingerprint {
        kind: "claude".into(),
        command: vec!["claude".into()],
        env: BTreeMap::new(),
    };
    let refused = profiletrust::record_launched(plane_dir.path(), "p", &print)
        .expect_err("a record that resolves outside the plane is refused");

    assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(
        refused.to_string().contains("outside the plane"),
        "refused by containment: {refused}"
    );
    assert_eq!(
        std::fs::read_to_string(outside.path().join("record.json")).unwrap(),
        "{}\n",
        "and the file outside is untouched"
    );
}

#[test]
fn a_single_todo_linked_out_of_the_plane_is_not_read_by_the_duplicate_check() {
    charter_core::unsteered!();
    // The STORE is legitimate; one entry in it is a committed link out. Gating only the
    // directory left every entry ungated, so this file was read, its heading echoed back by
    // `ws todo`'s refusal, and its body used as the comparison oracle.
    let (_plane_dir, outside, plane) = plane_and_outside();
    std::fs::write(
        outside.path().join("secret.md"),
        "# Board minutes: layoffs in Q3\n\n_2026-03-02 09:14 · persistent_\n\nCONFIDENTIAL\n",
    )
    .unwrap();
    let ws = plane.workspace("alpha").unwrap();
    let store = ws.dir().join("todos");
    std::fs::create_dir_all(&store).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.md"), store.join("leak.md")).unwrap();

    assert_eq!(
        charter_core::memstore::duplicate_of(plane.root(), &store, "Board minutes: layoffs in Q3"),
        None,
        "an entry outside the plane is not an existing todo"
    );

    // And the write it was blocking goes through.
    ws.add_todo("Board minutes: layoffs in Q3", pinned())
        .unwrap();
    assert_eq!(ws.todos().unwrap().len(), 1);
}
