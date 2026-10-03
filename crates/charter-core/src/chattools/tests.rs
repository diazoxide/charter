use chrono::NaiveDate;
use serde_json::json;

use super::*;

/// A project with `charter.toml` and the named workspaces, each with charter's own template.
fn project(workspaces: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "").unwrap();
    for ws in workspaces {
        let found = crate::workspaces::Plane::open(dir.path())
            .workspace(ws)
            .unwrap();
        std::fs::create_dir_all(found.dir()).unwrap();
        found.scaffold_charter().unwrap();
    }
    dir
}

fn at(h: u32, m: u32) -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 3)
        .unwrap()
        .and_hms_opt(h, m, 0)
        .unwrap()
}

fn in_ws(name: &str) -> Place {
    Place::Workspace(name.to_owned())
}

fn args(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn call_ok(root: &std::path::Path, place: &Place, tool: &str, given: serde_json::Value) -> String {
    call(root, place, tool, &args(given), at(9, 0))
        .unwrap_or_else(|why| panic!("{tool} refused: {why}"))
}

#[test]
fn a_todo_added_through_the_tools_is_listed_in_the_chat_s_workspace() {
    let p = project(&["alpha"]);
    let said = call_ok(
        p.path(),
        &in_ws("alpha"),
        "todo_add",
        json!({"text": "Ship the MCP server"}),
    );
    assert!(said.contains("alpha"), "{said}");

    let listed = call_ok(p.path(), &in_ws("alpha"), "todo_list", json!({}));
    assert!(listed.contains("Ship the MCP server"), "{listed}");
}

#[test]
fn a_todo_in_another_workspace_is_out_of_the_chat_s_reach() {
    let p = project(&["alpha", "beta"]);
    call_ok(
        p.path(),
        &in_ws("beta"),
        "todo_add",
        json!({"text": "Beta's own work"}),
    );

    let listed = call_ok(p.path(), &in_ws("alpha"), "todo_list", json!({}));
    assert!(!listed.contains("Beta's own work"), "{listed}");
}

#[test]
fn no_tool_takes_a_workspace_a_project_or_a_path_as_an_argument() {
    // The chat's scope is where it works, never what the model names (HP-7).
    for tool in &TOOLS {
        let schema = (tool.schema)();
        let properties = schema["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for key in properties.keys() {
            assert!(
                ![
                    "workspace",
                    "plane",
                    "project",
                    "root",
                    "path",
                    "dir",
                    "cwd"
                ]
                .contains(&key.as_str()),
                "{} takes {key}",
                tool.name
            );
        }
    }
}

#[test]
fn the_tools_are_the_ones_charter_names_and_each_says_what_it_does() {
    let names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "todo_list",
            "todo_add",
            "todo_done",
            "memory_search",
            "memory_add",
            "session_record_list",
            "session_record_read",
            "change_status",
            "ask_operator",
        ]
    );
    for tool in &TOOLS {
        assert!(!tool.description.is_empty(), "{}", tool.name);
        assert_eq!((tool.schema)()["type"], "object", "{}", tool.name);
    }
}

#[test]
fn closing_a_todo_leaves_its_trace_in_the_journal() {
    let p = project(&["alpha"]);
    call_ok(
        p.path(),
        &in_ws("alpha"),
        "todo_add",
        json!({"text": "Write the tests"}),
    );
    let ws = crate::workspaces::Plane::open(p.path())
        .workspace("alpha")
        .unwrap();
    let slug = ws.todos().unwrap()[0].slug.clone();

    call_ok(
        p.path(),
        &in_ws("alpha"),
        "todo_done",
        json!({"slug": slug}),
    );

    assert!(ws.todos().unwrap().is_empty());
    assert!(
        ws.memories()
            .unwrap()
            .iter()
            .any(|m| m.title == "Closed todo: Write the tests")
    );
}

#[test]
fn closing_a_todo_that_is_not_there_is_refused_by_name() {
    let p = project(&["alpha"]);
    let refused = call(
        p.path(),
        &in_ws("alpha"),
        "todo_done",
        &args(json!({"slug": "no-such-todo"})),
        at(9, 0),
    )
    .unwrap_err();
    assert!(refused.contains("no-such-todo"), "{refused}");
}

#[test]
fn a_slug_that_walks_out_of_the_todos_is_refused() {
    let p = project(&["alpha"]);
    std::fs::write(p.path().join("charter.toml.bak"), "x").unwrap();
    let refused = call(
        p.path(),
        &in_ws("alpha"),
        "todo_done",
        &args(json!({"slug": "../../../charter"})),
        at(9, 0),
    );
    assert!(refused.is_err(), "{refused:?}");
    assert!(p.path().join("charter.toml").exists());
}

#[test]
fn a_memory_added_through_the_tools_is_found_by_a_search() {
    let p = project(&["alpha"]);
    call_ok(
        p.path(),
        &in_ws("alpha"),
        "memory_add",
        json!({"text": "The sandbox denies the vaults to every chat"}),
    );

    let found = call_ok(
        p.path(),
        &in_ws("alpha"),
        "memory_search",
        json!({"query": "vaults"}),
    );
    assert!(found.contains("The sandbox denies the vaults"), "{found}");
    let missed = call_ok(
        p.path(),
        &in_ws("alpha"),
        "memory_search",
        json!({"query": "zebra"}),
    );
    assert!(!missed.contains("sandbox"), "{missed}");
}

#[test]
fn a_tool_that_needs_a_workspace_says_so_at_the_project_root() {
    let p = project(&[]);
    let refused = call(
        p.path(),
        &Place::PlaneRoot,
        "todo_list",
        &args(json!({})),
        at(9, 0),
    )
    .unwrap_err();
    assert!(refused.contains("workspace"), "{refused}");
}

#[test]
fn a_workspace_that_is_not_there_is_never_created_by_a_tool() {
    let p = project(&[]);
    let refused = call(
        p.path(),
        &in_ws("ghost"),
        "todo_add",
        &args(json!({"text": "Haunt"})),
        at(9, 0),
    );
    assert!(refused.is_err());
    assert!(!p.path().join("workspaces/ghost").exists());
}

#[test]
fn a_missing_argument_is_named() {
    let p = project(&["alpha"]);
    let refused = call(
        p.path(),
        &in_ws("alpha"),
        "todo_add",
        &args(json!({})),
        at(9, 0),
    )
    .unwrap_err();
    assert!(refused.contains("text"), "{refused}");
}

#[test]
fn a_tool_charter_does_not_have_is_refused_by_name() {
    let p = project(&["alpha"]);
    let refused = call(
        p.path(),
        &in_ws("alpha"),
        "vault_read",
        &args(json!({})),
        at(9, 0),
    )
    .unwrap_err();
    assert!(refused.contains("vault_read"), "{refused}");
}

#[test]
fn session_records_are_listed_and_read_where_the_chat_works() {
    let p = project(&["alpha"]);
    let recorded = crate::sessionrecord::record(
        p.path(),
        &crate::sessionrecord::New {
            title: "Built the server",
            body: "## Goal\n\nG.\n\n## Done\n\n- d\n\n## Decisions\n\n- x\n\n## Open\n\nNothing.\n\n## How to resume\n\nRead it.\n",
            facts: &crate::sessionrecord::Facts {
                place: in_ws("alpha"),
                at: at(8, 0),
                chat: None,
                persona: None,
                pieces: Vec::new(),
            },
        },
    )
    .unwrap();
    let file = recorded
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let listed = call_ok(p.path(), &in_ws("alpha"), "session_record_list", json!({}));
    assert!(
        listed.contains("Built the server") && listed.contains(&file),
        "{listed}"
    );

    let read = call_ok(
        p.path(),
        &in_ws("alpha"),
        "session_record_read",
        json!({"file": file}),
    );
    assert!(read.contains("Read it."), "{read}");
}

#[test]
fn a_session_record_is_read_by_its_file_name_and_never_by_a_path() {
    let p = project(&["alpha"]);
    let refused = call(
        p.path(),
        &in_ws("alpha"),
        "session_record_read",
        &args(json!({"file": "../../charter.toml"})),
        at(9, 0),
    );
    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn change_status_reads_the_record_and_never_asks_the_forge() {
    let p = project(&["alpha"]);
    let said = call_ok(p.path(), &in_ws("alpha"), "change_status", json!({}));
    assert!(said.contains("No changes"), "{said}");
}

#[test]
fn ask_operator_is_answered_by_the_server_and_not_by_call() {
    // It needs the harness to ask the person, which only the server that holds the
    // connection can do.
    let p = project(&["alpha"]);
    let refused = call(
        p.path(),
        &in_ws("alpha"),
        ASK_OPERATOR,
        &args(json!({"question": "Merge?"})),
        at(9, 0),
    )
    .unwrap_err();
    assert!(refused.contains(ASK_OPERATOR), "{refused}");
}

#[test]
fn a_question_for_the_operator_needs_words() {
    assert!(question(&args(json!({"question": "  "}))).is_err());
    assert_eq!(
        question(&args(json!({"question": " Merge it? "}))).unwrap(),
        "Merge it?"
    );
}

// --- V74: every store and entry inside `workspaces/<ws>/`, with no link on the way ----------- //

/// Every file under `dir`, as `(path, bytes)`, so a test can say nothing was written.
fn snapshot(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = std::fs::symlink_metadata(&path).unwrap().file_type();
        if kind.is_dir() {
            out.extend(snapshot(&path));
        } else if kind.is_file() {
            out.push((path.clone(), std::fs::read(&path).unwrap()));
        }
    }
    out.sort();
    out
}

/// What each tool touches, and a call that would write or read there.
fn calls_on(store: &str) -> Vec<(&'static str, serde_json::Value)> {
    match store {
        "todos" => vec![
            ("todo_list", json!({})),
            ("todo_add", json!({"text": "Planted through a link"})),
            ("todo_done", json!({"slug": "their-todo"})),
        ],
        "memory" => vec![
            ("memory_search", json!({})),
            ("memory_add", json!({"text": "Planted through a link"})),
            ("todo_done", json!({"slug": "mine"})),
        ],
        "sessions" => vec![
            ("session_record_list", json!({})),
            (
                "session_record_read",
                json!({"file": "20261003-080000-theirs.md"}),
            ),
        ],
        "changes" => vec![("change_status", json!({}))],
        other => panic!("no store {other}"),
    }
}

/// A project where `beta` and a persona each hold `store`, with a file in it.
fn project_with_targets(store: &str) -> tempfile::TempDir {
    let p = project(&["alpha", "beta"]);
    for target in ["workspaces/beta", "personas/devops"] {
        let dir = p.path().join(target).join(store);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("their-todo.md"),
            "# Theirs\n\n_2026-10-01 09:00 · todo_\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("20261003-080000-theirs.md"),
            "---\ntitle: theirs\n---\n",
        )
        .unwrap();
        std::fs::write(dir.join("theirs.json"), "{}").unwrap();
    }
    // alpha's own todo, so `todo_done` has something real to close when the memory is linked.
    if store == "memory" {
        let ws = crate::workspaces::Plane::open(p.path())
            .workspace("alpha")
            .unwrap();
        std::fs::create_dir_all(ws.dir().join("todos")).unwrap();
        ws.add_todo("Mine", at(8, 0)).unwrap();
        let slug = ws.todos().unwrap()[0].slug.clone();
        std::fs::rename(
            ws.dir().join("todos").join(format!("{slug}.md")),
            ws.dir().join("todos/mine.md"),
        )
        .unwrap();
    }
    p
}

#[cfg(unix)]
fn refused_with_nothing_written(p: &tempfile::TempDir, store: &str) {
    let before = snapshot(p.path());
    for (tool, given) in calls_on(store) {
        let answered = call(p.path(), &in_ws("alpha"), tool, &args(given), at(9, 0));
        assert!(
            answered.is_err(),
            "{tool} on a linked {store} answered {answered:?}"
        );
    }
    assert_eq!(
        snapshot(p.path()),
        before,
        "something was written for {store}"
    );
}

#[cfg(unix)]
#[test]
fn a_store_that_is_a_link_to_another_workspace_or_a_persona_is_refused() {
    for store in ["todos", "memory", "sessions", "changes"] {
        for target in ["../beta", "../../personas/devops"] {
            let p = project_with_targets(store);
            std::os::unix::fs::symlink(
                format!("{target}/{store}"),
                p.path().join("workspaces/alpha").join(store),
            )
            .unwrap();
            refused_with_nothing_written(&p, store);
        }
    }
}

#[cfg(unix)]
#[test]
fn a_store_holding_a_link_to_a_file_outside_is_refused() {
    for store in ["todos", "memory", "sessions", "changes"] {
        let p = project_with_targets(store);
        let mine = p.path().join("workspaces/alpha").join(store);
        std::fs::create_dir_all(&mine).unwrap();
        for name in ["their-todo.md", "20261003-080000-theirs.md", "theirs.json"] {
            std::os::unix::fs::symlink(
                p.path().join("workspaces/beta").join(store).join(name),
                mine.join(name),
            )
            .unwrap();
        }
        refused_with_nothing_written(&p, store);
    }
}

#[cfg(unix)]
#[test]
fn a_store_holding_a_hard_link_to_a_file_outside_is_refused() {
    let p = project_with_targets("memory");
    let mine = p.path().join("workspaces/alpha/memory");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::hard_link(
        p.path().join("personas/devops/memory/their-todo.md"),
        mine.join("MEMORY.md"),
    )
    .unwrap();
    refused_with_nothing_written(&p, "memory");
}

#[test]
fn a_write_tool_rereads_whether_the_project_is_writable() {
    // FR-24: the server lives as long as the chat, and the project can move under it.
    let p = project(&["alpha"]);
    call_ok(
        p.path(),
        &in_ws("alpha"),
        "todo_add",
        json!({"text": "Before"}),
    );
    std::fs::write(p.path().join("charter.toml"), "schema = 999\n").unwrap();
    let before = snapshot(p.path());

    let refused = call(
        p.path(),
        &in_ws("alpha"),
        "memory_add",
        &args(json!({"text": "After"})),
        at(9, 0),
    )
    .unwrap_err();
    assert!(refused.contains("999"), "{refused}");
    assert_eq!(snapshot(p.path()), before);
    // A read still answers.
    call_ok(p.path(), &in_ws("alpha"), "todo_list", json!({}));
}

#[test]
fn the_operator_reads_who_asks_and_that_no_secret_goes_there() {
    assert_eq!(asked("Merge it?"), "This chat asks: Merge it?");
}

#[test]
fn a_retried_question_is_bound_to_the_one_that_was_asked() {
    assert_eq!(question_state("Merge it?"), question_state("Merge it?"));
    assert_ne!(question_state("Merge it?"), question_state("Delete it?"));
    assert!(question_state("Merge it?").starts_with("ask_operator:"));
}

/// V74 under a race: a thread swaps the chat's real memory directory with a link to a
/// persona's memory, atomically and as fast as it can, while the tool writes. Measured before
/// the stores were held by descriptor: 34 of 1,500 writes landed in the persona.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_store_swapped_for_a_link_while_the_tool_runs_never_carries_a_write_outside() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const CALLS: usize = 2_000;
    let p = project(&["alpha", "beta"]);
    let persona = p.path().join("personas/devops/memory");
    std::fs::create_dir_all(&persona).unwrap();
    std::fs::write(persona.join("MEMORY.md"), "# Persona memory\n").unwrap();
    let alpha = p.path().join("workspaces/alpha");
    std::fs::create_dir_all(alpha.join("memory")).unwrap();
    std::os::unix::fs::symlink("../../personas/devops/memory", alpha.join("swap")).unwrap();
    let outside = || {
        (
            snapshot(&p.path().join("personas")),
            snapshot(&p.path().join("workspaces/beta")),
        )
    };
    let before = outside();

    let stop = Arc::new(AtomicBool::new(false));
    let swaps = Arc::new(AtomicUsize::new(0));
    let racer = {
        let (stop, swaps) = (stop.clone(), swaps.clone());
        let (from, to) = (alpha.join("memory"), alpha.join("swap"));
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if rustix::fs::renameat_with(
                    rustix::fs::CWD,
                    &from,
                    rustix::fs::CWD,
                    &to,
                    rustix::fs::RenameFlags::EXCHANGE,
                )
                .is_ok()
                {
                    swaps.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
    };
    let mut written = 0;
    for i in 0..CALLS {
        let said = call(
            p.path(),
            &in_ws("alpha"),
            "memory_add",
            &args(json!({"text": format!("Raced note {i}")})),
            at(9, 0),
        );
        if said.is_ok() {
            written += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();

    assert!(swaps.load(Ordering::Relaxed) > 0, "the racer never swapped");
    let after = outside();
    let landed: Vec<_> = after
        .0
        .iter()
        .chain(&after.1)
        .filter(|entry| !before.0.contains(entry) && !before.1.contains(entry))
        .map(|(path, _)| path.clone())
        .collect();
    assert!(
        landed.is_empty() && after == before,
        "{} files written or changed outside the workspace, the first: {:?}",
        landed.len(),
        landed.first()
    );
    // Every write the tool answered for is inside alpha, under whichever name its memory
    // directory has now.
    let inside = snapshot(&alpha)
        .iter()
        .filter(|(path, _)| path.to_string_lossy().contains("raced-note"))
        .count();
    assert_eq!(inside, written, "{written} answered, {inside} inside alpha");
}

/// `tool` called on another thread, answered within `seconds` or `None`.
#[cfg(unix)]
fn within(
    seconds: u64,
    root: &std::path::Path,
    tool: &'static str,
    given: serde_json::Value,
) -> Option<Result<String, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(call(&root, &in_ws("alpha"), tool, &args(given), at(9, 0)));
    });
    rx.recv_timeout(std::time::Duration::from_secs(seconds))
        .ok()
}

#[cfg(unix)]
#[test]
fn a_fifo_in_a_store_is_refused_at_once_and_the_store_is_let_go() {
    for (store, tool, given) in [
        ("memory", "memory_add", json!({"text": "Never written"})),
        ("memory", "memory_search", json!({})),
        ("todos", "todo_add", json!({"text": "Never written"})),
    ] {
        let p = project(&["alpha"]);
        let dir = p.path().join("workspaces/alpha").join(store);
        std::fs::create_dir_all(&dir).unwrap();
        let made = crate::forklock::status(
            std::process::Command::new("mkfifo").arg(dir.join("MEMORY.md")),
        )
        .expect("mkfifo runs");
        assert!(made.success());

        let answered = within(5, p.path(), tool, given).expect("answered, not hung");
        let why = answered.expect_err("refused");
        assert!(
            why.contains(store) && !why.contains(&p.path().display().to_string()),
            "{why}"
        );
        // The store's lock is let go: a `charter` command can take it.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _held = crate::rewrite::Lock::on(&dir);
            let _ = tx.send(());
        });
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("the store's lock was let go");
    }
}

#[test]
fn every_tool_pre_allowed_in_claude_code_is_one_that_only_reads() {
    // V79: the pre-allowed tools are the five reads, each a tool the server offers and marks
    // read-only, and never `ask_operator`, which is marked read-only and still asks.
    assert_eq!(
        PRE_ALLOWED,
        [
            "todo_list",
            "memory_search",
            "session_record_list",
            "session_record_read",
            "change_status",
        ]
    );
    for name in PRE_ALLOWED {
        let tool = TOOLS
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("{name} is not a tool"));
        assert!(tool.read_only, "{name} writes");
        assert_ne!(name, ASK_OPERATOR);
    }
}
