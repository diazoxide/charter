//! Every public entry point, against a plane where the path it touches is a link out.
//!
//! This test exists because the same mistake was made three times in a row: the gate was put
//! one level above the thing actually opened — on the workspace instead of the file, on the
//! store instead of the index, on the name instead of the resolved path. Each round passed
//! every check and each round left a hole. So this asks the only question that matters, of
//! the whole surface at once: **after calling everything, is anything outside the plane
//! different?**
//!
//! It is deliberately not a unit test per rule. A rule can be right while the call site that
//! needed it is missing, which is exactly what kept happening.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A directory outside the plane, and what it held before charter ran.
struct Outside {
    dir: tempfile::TempDir,
    before: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

impl Outside {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // A file to overwrite, a file to delete, and a name to create.
        std::fs::write(dir.path().join("precious"), b"PRECIOUS\n").unwrap();
        std::fs::write(dir.path().join("deletable.md"), b"DELETABLE\n").unwrap();
        std::fs::create_dir_all(dir.path().join("store")).unwrap();
        std::fs::write(
            dir.path().join("store/secret.md"),
            b"# Secret\n\nSECRET BODY\n",
        )
        .unwrap();
        let before = snapshot(dir.path());
        Self { dir, before }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Everything that changed out here, as lines to print.
    fn changed(&self) -> Vec<String> {
        let now = snapshot(self.path());
        let mut out = Vec::new();
        for (path, was) in &self.before {
            match now.get(path) {
                None => out.push(format!("DELETED {}", path.display())),
                Some(is) if is != was => out.push(format!("CHANGED {}", path.display())),
                _ => {}
            }
        }
        for path in now.keys() {
            if !self.before.contains_key(path) {
                out.push(format!("CREATED {}", path.display()));
            }
        }
        out
    }
}

/// Every path under `root`, with its bytes (or `None` for a directory).
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut found = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                found.insert(rel, None);
                stack.push(path);
            } else {
                found.insert(rel, std::fs::read(&path).ok());
            }
        }
    }
    found
}

fn stamp() -> chrono::NaiveDateTime {
    "2026-03-02T09:14:00".parse().unwrap()
}

/// A plane with `alpha` and a `devops` persona, both scaffolded for real.
fn plane() -> (tempfile::TempDir, charter_core::workspaces::Plane) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(dir.path().join("workspaces/alpha")).unwrap();
    std::fs::create_dir_all(dir.path().join("personas/devops")).unwrap();
    let plane = charter_core::workspaces::Plane::open(dir.path());
    (dir, plane)
}

/// Call every public entry point that reads or writes. Failures are expected and ignored —
/// what is under test is the filesystem afterwards, not the return values.
fn exercise_everything(plane: &charter_core::workspaces::Plane) {
    if let Ok(ws) = plane.workspace("alpha") {
        let _ = ws.scaffold_charter();
        let _ = ws.set_vision("a vision");
        let _ = ws.vision();
        let _ = ws.manifest();
        let _ = ws.write_manifest(&serde_json::from_str(r#"{"name":"alpha"}"#).unwrap());
        let _ = ws.remember("a durable fact", stamp());
        let _ = ws.add_todo("a todo", stamp());
        let _ = ws.memories();
        let _ = ws.todos();
        for slug in ["a-todo", "deletable", "../deletable", "secret"] {
            let _ = ws.close_todo(slug, stamp());
            let _ = charter_core::memstore::forget(plane.root(), &ws.dir().join("todos"), slug);
            let _ = charter_core::memstore::forget(plane.root(), &ws.dir().join("memory"), slug);
        }
        let _ =
            charter_core::memstore::duplicate_of(plane.root(), &ws.dir().join("todos"), "a todo");
        // `write` straight into a store, WITHOUT `ensure_index` first. It is public and the
        // index gate is the only one on this path — reaching it only through the workspace
        // made that gate look redundant when it is not.
        for store in ["memory", "todos"] {
            let _ = charter_core::memstore::write(
                plane.root(),
                &ws.dir().join(store),
                "written straight in",
                None,
                true,
                "persistent",
                true,
                stamp(),
            );
        }
    }
    if let Ok(persona) = plane.persona("devops") {
        let _ = persona.scaffold_memory();
        let _ = persona.remember("a persona fact", stamp());
        let _ = persona.role();
        let _ = persona.memories();
    }
    let _ = plane.workspaces();
    let _ = plane.personas();
    let _ = plane.default_persona();
    let print = charter_core::profiletrust::Fingerprint {
        kind: "claude".into(),
        command: vec!["claude".into()],
        env: BTreeMap::new(),
    };
    let _ = charter_core::profiletrust::record_launched(plane.root(), "p", &print);
    let _ = charter_core::profiletrust::last_launched(plane.root(), "p");
}

/// Run the whole surface with `link` pointing at the outside directory (or a file in it).
fn with_link_out(link: &str, target_is_file: bool) -> Vec<String> {
    let (dir, plane) = plane();
    let outside = Outside::new();
    let at = dir.path().join(link);
    if let Some(parent) = at.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let _ = std::fs::remove_file(&at);
    let _ = std::fs::remove_dir_all(&at);
    let target = if target_is_file {
        outside.path().join("precious")
    } else {
        outside.path().to_path_buf()
    };
    std::os::unix::fs::symlink(&target, &at).unwrap();

    exercise_everything(&plane);
    outside.changed()
}

#[test]
fn nothing_outside_the_plane_is_touched_however_the_link_is_placed() {
    // One case per path a public entry point opens. Each was a hole at some point, or is one
    // level away from one that was.
    let places: [(&str, bool); 12] = [
        ("workspaces/alpha", false),
        ("workspaces/alpha/workspace.md", true),
        ("workspaces/alpha/workspace.json", true),
        ("workspaces/alpha/memory", false),
        ("workspaces/alpha/memory/MEMORY.md", true),
        ("workspaces/alpha/todos", false),
        ("workspaces/alpha/todos/MEMORY.md", true),
        ("workspaces", false),
        ("personas/devops", false),
        ("personas/devops/persona.md", true),
        ("personas/devops/memory", false),
        (".charter", false),
    ];

    let mut broken = Vec::new();
    for (place, is_file) in places {
        let changed = with_link_out(place, is_file);
        if !changed.is_empty() {
            broken.push(format!("  {place} -> {changed:?}"));
        }
    }

    assert!(
        broken.is_empty(),
        "these links let charter reach outside the plane:\n{}",
        broken.join("\n")
    );
}

#[test]
fn a_dangling_link_creates_nothing_outside_the_plane() {
    // `canonicalize` fails for a dangling link, which is how one got judged by its own name.
    for place in [
        "workspaces/alpha/memory/MEMORY.md",
        "workspaces/alpha/todos/MEMORY.md",
        "workspaces/alpha/workspace.md",
        "workspaces/alpha/workspace.json",
        "personas/devops/memory/MEMORY.md",
    ] {
        let (dir, plane) = plane();
        let outside = tempfile::tempdir().unwrap();
        let at = dir.path().join(place);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(outside.path().join("planted"), &at).unwrap();

        exercise_everything(&plane);

        assert_eq!(
            std::fs::read_dir(outside.path()).unwrap().count(),
            0,
            "{place}: a dangling link had its target created"
        );
    }
}

#[test]
fn the_plane_itself_is_still_written_when_nothing_is_linked_out() {
    // The guard against a gate so strict it stops charter working at all.
    let (dir, plane) = plane();

    exercise_everything(&plane);

    let ws = dir.path().join("workspaces/alpha");
    assert!(ws.join("workspace.md").is_file(), "the charter was written");
    assert!(
        ws.join("memory/MEMORY.md").is_file(),
        "the journal was written"
    );
    assert!(
        dir.path()
            .join("personas/devops/memory/MEMORY.md")
            .is_file(),
        "the persona's index was written"
    );
    assert!(
        dir.path()
            .join(".charter")
            .join(charter_core::profiletrust::RECORD)
            .is_file(),
        "the consent record was written"
    );
}
