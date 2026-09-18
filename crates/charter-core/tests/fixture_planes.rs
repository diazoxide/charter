//! Checks against the committed fixture planes, which the Python charter itself wrote
//! (`tests/fixtures/planes/generate.py`). They are the oracle that needs no Python to read:
//! whatever charter put in these files is what a Rust reader has to agree with.

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
    assert_eq!(daily().workspaces().unwrap(), vec!["alpha", "beta"]);
}

#[test]
fn a_dotted_directory_under_workspaces_is_never_a_workspace() {
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
    assert_eq!(daily().workspace("alpha").vision(), "Ship the widget");
    assert_eq!(
        daily().workspace("beta").vision(),
        "Retire the old importer"
    );
}

#[test]
fn an_unset_vision_reads_as_empty_rather_than_as_its_placeholder() {
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

    assert_eq!(plane.workspace("fresh").vision(), "");
}

#[test]
fn the_open_todos_of_a_workspace_are_the_files_its_index_lists() {
    let todos = daily().workspace("alpha").todos().unwrap();

    assert_eq!(todos.len(), 1, "one todo is open; the other was closed");
    assert_eq!(todos[0].title, "Review the rollout plan");
    assert_eq!(todos[0].slug, "20260302-091400-review-the-rollout-plan");
    assert_eq!(todos[0].body, "Review the rollout plan");
}

#[test]
fn a_workspace_that_was_never_given_a_todo_has_none_rather_than_failing() {
    assert_eq!(daily().workspace("beta").todos().unwrap(), vec![]);
}

#[test]
fn the_memories_of_a_workspace_come_back_in_the_order_their_names_give() {
    let memories = daily().workspace("alpha").memories().unwrap();

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
    // `_shared` and `_dispatch` are charter's own, not personas.
    assert_eq!(daily().personas().unwrap(), vec!["devops", "steward"]);
}

#[test]
fn an_underscore_directory_is_not_a_persona_even_when_it_holds_a_persona_file() {
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
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");

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
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");
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
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");

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
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");

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
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");
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
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");

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
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");
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
fn writing_a_manifest_leaves_no_temp_file_behind() {
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");
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
    // `charter ws todo done write-the-migration` is how the fixture generator closes one.
    let (_tmp, plane) = temp_plane();
    let ws = plane.workspace("alpha");
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
    assert_eq!(daily().default_persona(), Some("steward".to_string()));
}

#[test]
fn a_plane_whose_manifest_names_no_persona_has_no_default() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();

    assert_eq!(
        charter_core::workspaces::Plane::open(dir.path()).default_persona(),
        None
    );
}

#[test]
fn an_unparseable_charter_toml_names_no_persona_rather_than_failing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = = =\n").unwrap();

    assert_eq!(
        charter_core::workspaces::Plane::open(dir.path()).default_persona(),
        None
    );
}

#[test]
fn a_path_inside_a_workspace_names_that_workspace() {
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
    let plane = daily();
    let root = plane.root().to_path_buf();

    assert_eq!(plane.workspace_of(&root), None);
    assert_eq!(plane.workspace_of(&root.join("personas/devops")), None);
    assert_eq!(plane.workspace_of(Path::new("/tmp")), None);
}

#[test]
fn a_path_naming_a_workspace_that_is_not_there_belongs_to_none() {
    let plane = daily();

    assert_eq!(
        plane.workspace_of(&plane.root().join("workspaces/ghost/x")),
        None
    );
}
