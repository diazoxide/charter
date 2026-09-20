//! `recall` reads every entry of every base in scope. That is `duplicate_of`'s shape, and
//! `duplicate_of` was M1.1's last containment hole: a committed `leak.md ->
//! /outside/secret.md` was read, its heading echoed, and its body used as a similarity
//! oracle for the rest of the file.
//!
//! So this plants a link out of the plane at every place a read reaches — an entry, a
//! store, a refs directory, a refs subdirectory — each pointing at a file whose heading and
//! body match the query, and asks the read side the only question that matters: does
//! anything that came back, or any score, come from outside?

use std::path::{Path, PathBuf};

use charter_core::recall::{self, Ask, Workspaces};

const SECRET: &str = "# Secret mondays\n\n_2026-03-02 09:14 · persistent_\n\nSECRET mondays\n";

fn plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("plane");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    for store in [
        "workspaces/alpha/memory",
        "personas/devops/memory",
        "personas/devops/refs",
        "personas/_shared/memory",
        "personas/_shared/refs",
    ] {
        std::fs::create_dir_all(root.join(store)).unwrap();
    }
    std::fs::write(
        root.join("personas/devops/persona.md"),
        "---\nrole: ops\n---\n",
    )
    .unwrap();
    std::fs::write(
        root.join("workspaces/alpha/memory/20260302-091400-honest.md"),
        "# Honest mondays\n\n_2026-03-02 09:14 · persistent_\n\nhonest mondays\n",
    )
    .unwrap();
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(outside.join("store")).unwrap();
    std::fs::write(outside.join("secret.md"), SECRET).unwrap();
    std::fs::write(outside.join("store/secret.md"), SECRET).unwrap();
    (dir, root)
}

fn ask(query: Option<&str>) -> Ask {
    Ask {
        scopes: recall::SCOPES.map(str::to_string).to_vec(),
        workspaces: Some(Workspaces::One("alpha".into())),
        persona: Some("devops".into()),
        session: "s1".into(),
        query: query.map(str::to_string),
        limit: 0,
        since: None,
    }
}

/// Everything recall returned, as `path — title`.
fn seen(root: &Path, query: Option<&str>) -> Vec<String> {
    recall::recall(root, &ask(query))
        .hits
        .iter()
        .map(|h| format!("{} — {}", h.path.display(), h.title))
        .collect()
}

#[test]
fn nothing_outside_the_plane_is_read_however_the_link_is_placed() {
    // (where the link goes, whether it points at the outside FILE or the outside STORE)
    let places: [(&str, bool); 8] = [
        ("workspaces/alpha/memory/leak.md", true),
        ("personas/devops/memory/leak.md", true),
        ("personas/_shared/memory/leak.md", true),
        ("personas/devops/refs/leak.md", true),
        ("personas/devops/refs/linked", false),
        ("personas/_shared/refs/linked", false),
        ("personas/devops/memory", false),
        ("workspaces/alpha/memory", false),
    ];
    let mut broken = Vec::new();
    for (place, is_file) in places {
        for query in [Some("secret"), Some("mondays"), None] {
            let (dir, root) = plane();
            let at = root.join(place);
            let _ = std::fs::remove_dir_all(&at);
            let target = if is_file {
                dir.path().join("outside/secret.md")
            } else {
                dir.path().join("outside/store")
            };
            std::os::unix::fs::symlink(&target, &at).unwrap();

            let leaked: Vec<String> = seen(&root, query)
                .into_iter()
                .filter(|hit| hit.contains("Secret") || hit.contains("outside"))
                .collect();
            if !leaked.is_empty() {
                broken.push(format!("  {place} ({query:?}) -> {leaked:?}"));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "recall read through these links:\n{}",
        broken.join("\n")
    );
}

#[test]
fn the_plane_itself_is_still_read_when_nothing_is_linked_out() {
    // The guard against a gate so strict recall finds nothing at all.
    let (_dir, root) = plane();

    assert_eq!(
        seen(&root, Some("mondays")),
        vec![format!(
            "{} — Honest mondays",
            root.join("workspaces/alpha/memory/20260302-091400-honest.md")
                .display()
        )]
    );
}

#[test]
fn a_link_that_lands_inside_the_plane_is_read_and_labelled_by_where_it_lands() {
    // charter follows a link that stays inside the plane's data; a search labels the hit
    // by the base it resolves into, a listing by the base it was listed from.
    let (_dir, root) = plane();
    std::fs::write(
        root.join("personas/_shared/memory/shared-fact.md"),
        "# Shared fact\n\n_2026-03-01 09:14 · persistent_\n\nshared kiwi\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        root.join("personas/_shared/memory/shared-fact.md"),
        root.join("workspaces/alpha/memory/inside.md"),
    )
    .unwrap();

    let found = recall::recall(&root, &ask(Some("kiwi"))).hits;
    let labels: Vec<(&str, String)> = found
        .iter()
        .map(|h| {
            (
                h.label.as_str(),
                h.path.file_name().unwrap().to_string_lossy().into_owned(),
            )
        })
        .collect();

    assert_eq!(
        labels,
        // One score each, so the path decides the order: `personas/…` before `workspaces/…`.
        vec![
            ("shared", "shared-fact.md".to_string()),
            ("shared", "inside.md".to_string())
        ]
    );
    let listed = recall::recall(&root, &ask(None)).hits;
    assert!(
        listed
            .iter()
            .any(|h| h.label == "workspace:alpha" && h.path.ends_with("inside.md")),
        "{listed:?}"
    );
}

#[test]
fn a_fifo_or_an_oversized_file_is_not_a_memory() {
    // A FIFO blocks a reader for ever, which is why the gate asks `lstat`/`stat` and never
    // opens anything first. If this hangs, the gate opened it.
    let (_dir, root) = plane();
    let store = root.join("workspaces/alpha/memory");
    let made = charter_core::forklock::status(
        std::process::Command::new("mkfifo").arg(store.join("pipe.md")),
    )
    .unwrap();
    assert!(made.success());
    std::fs::write(
        store.join("big.md"),
        format!("# Big mondays\n{}", "x".repeat(1_048_577)),
    )
    .unwrap();

    let titles: Vec<String> = recall::recall(&root, &ask(None))
        .hits
        .into_iter()
        .map(|h| h.title)
        .collect();

    assert_eq!(titles, vec!["Honest mondays"]);
}
