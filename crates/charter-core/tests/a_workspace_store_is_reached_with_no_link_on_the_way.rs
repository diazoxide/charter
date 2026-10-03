//! V74 for the `charter` CLI and the app (#1064): a workspace's todos, memory, session records
//! and changes are reached from the project with no link anywhere on the way, and used through
//! the directory that was held, so a link a chat plants in its own workspace never carries one
//! of the operator's writes, or reads, into another workspace or a persona.
//!
//! These are the core calls the CLI's commands and the app's panels share (`Workspace`,
//! `memstore`, `sessionrecord`, `change::store`); the CLI's own words are pinned by its tests
//! and by the recorded behaviour.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use charter_core::active::Place;
use charter_core::change::store as changes;
use charter_core::memstore::Base;
use charter_core::sessionrecord;
use charter_core::workspaces::{Plane, Workspace};

fn at(h: u32, m: u32) -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 10, 3)
        .unwrap()
        .and_hms_opt(h, m, 0)
        .unwrap()
}

/// A project with workspaces `alpha` and `beta` and a persona, each of `beta` and the persona
/// holding a file of every kind each store keeps.
fn project() -> tempfile::TempDir {
    let p = tempfile::tempdir().unwrap();
    std::fs::write(p.path().join("charter.toml"), "schema = 1\n").unwrap();
    for ws in ["alpha", "beta"] {
        let found = Plane::open(p.path()).workspace(ws).unwrap();
        std::fs::create_dir_all(found.dir()).unwrap();
        found.scaffold_charter().unwrap();
    }
    for target in ["workspaces/beta", "personas/devops"] {
        for store in ["todos", "memory", "sessions", "changes"] {
            let dir = p.path().join(target).join(store);
            std::fs::create_dir_all(dir.join("archive")).unwrap();
            std::fs::write(dir.join("MEMORY.md"), "# Theirs\n\n- [Theirs](theirs.md)\n").unwrap();
            std::fs::write(
                dir.join("theirs.md"),
                "# Theirs\n\n_2026-10-01 09:00 · persistent_\n\nTheirs.\n",
            )
            .unwrap();
            std::fs::write(dir.join("archive/gone.md"), "# Gone\n\nGone.\n").unwrap();
            std::fs::write(dir.join("20261003-080000-theirs.md"), RECORD).unwrap();
            std::fs::write(dir.join("theirs.json"), CHANGE).unwrap();
        }
    }
    p
}

const RECORD: &str = "---\ntitle: Theirs\n---\n\n## Goal\n\nTheirs.\n";

const CHANGE: &str = r#"{"change": "theirs", "why": "theirs", "created": "2026-10-01", "by": "them", "members": [], "excluded": []}"#;

fn alpha(p: &tempfile::TempDir) -> Workspace {
    Plane::open(p.path()).workspace("alpha").unwrap()
}

/// Every file and link under `dir`, with its bytes or its target, so a test can say nothing
/// anywhere was written, moved or removed.
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
        } else if kind.is_file() {
            out.push((path.clone(), std::fs::read(&path).unwrap()));
        } else {
            // A FIFO: there, and never opened.
            out.push((path, b"fifo".to_vec()));
        }
    }
    out.sort();
    out
}

fn body() -> &'static str {
    "## Goal\n\nG.\n\n## Done\n\n- d\n\n## Decisions\n\n- x\n\n## Open\n\nNothing.\n\n## How to resume\n\nRead it.\n"
}

fn facts() -> sessionrecord::Facts {
    sessionrecord::Facts {
        place: Place::Workspace("alpha".to_owned()),
        at: at(9, 0),
        chat: None,
        persona: None,
        pieces: Vec::new(),
    }
}

fn a_change(slug: &str) -> charter_core::change::Record {
    charter_core::change::Record::parse(&CHANGE.replace("theirs", slug), slug).unwrap()
}

/// Each writer of `store`, run against alpha, with whether it answered Ok.
fn writers(p: &tempfile::TempDir, store: &str) -> Vec<(&'static str, bool)> {
    let ws = alpha(p);
    let root = p.path();
    match store {
        "todos" => vec![
            (
                "record_todo",
                ws.record_todo("Planted through a link", at(9, 0)).is_ok(),
            ),
            (
                "add_todo",
                ws.add_todo("Planted through a link", at(9, 0)).is_ok(),
            ),
            ("close_todo", ws.close_todo("theirs", at(9, 0)).is_ok()),
            ("forget_todo", ws.forget_todo("theirs").is_ok()),
        ],
        "memory" => vec![
            (
                "remember",
                ws.remember("Planted through a link", at(9, 0)).is_ok(),
            ),
            ("scaffold_memory", ws.scaffold_memory().is_ok()),
            (
                "edit_memory",
                ws.edit_memory("theirs", "Mine now", "Mine.", Base::Overwrite)
                    .is_ok(),
            ),
            ("archive_memory", ws.archive_memory("theirs").is_ok()),
            (
                "unarchive_memory",
                ws.unarchive_memory("gone", None).is_ok(),
            ),
            (
                "forget",
                charter_core::memstore::forget(root, &ws.dir().join("memory"), "theirs").is_ok(),
            ),
        ],
        "sessions" => vec![(
            "record",
            sessionrecord::record(
                root,
                &sessionrecord::New {
                    title: "Planted",
                    body: body(),
                    facts: &facts(),
                },
            )
            .is_ok(),
        )],
        "changes" => vec![
            (
                "write",
                changes::write(root, "alpha", &a_change("planted")).is_ok(),
            ),
            ("forget", changes::forget(root, "alpha", "theirs").is_ok()),
            (
                "landing",
                charter_core::change::landing::append(
                    root,
                    "alpha",
                    "laptop",
                    &charter_core::change::landing::Landing::new(
                        "theirs",
                        "api",
                        7,
                        &"a".repeat(40),
                        "b1",
                        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 10, 3, 9, 0, 0)
                            .unwrap(),
                    ),
                )
                .is_some(),
            ),
        ],
        other => panic!("no store {other}"),
    }
}

/// What alpha's readers of `store` see of the files `beta` and the persona hold.
fn readers_see_theirs(p: &tempfile::TempDir, store: &str) -> Vec<&'static str> {
    let ws = alpha(p);
    let root = p.path();
    let mut saw = Vec::new();
    let theirs = |text: &str| text.contains("Theirs") || text.contains("theirs");
    match store {
        "todos" => {
            if ws.todos().is_ok_and(|t| t.iter().any(|e| theirs(&e.title))) {
                saw.push("todos");
            }
        }
        "memory" => {
            if ws
                .memories()
                .is_ok_and(|m| m.iter().any(|e| theirs(&e.title)))
            {
                saw.push("memories");
            }
            if ws.open_memory("theirs").is_ok() {
                saw.push("open_memory");
            }
            let (found, _) = charter_core::memstore::read_entries(root, &ws.dir().join("memory"));
            if found.iter().any(|f| theirs(&f.title)) {
                saw.push("read_entries");
            }
        }
        "sessions" => {
            let place = Place::Workspace("alpha".to_owned());
            if sessionrecord::list(root, &place)
                .iter()
                .any(|r| theirs(&r.title))
            {
                saw.push("list");
            }
            if sessionrecord::show(root, &place, "20261003-080000-theirs.md").is_ok() {
                saw.push("show");
            }
        }
        "changes" => {
            if changes::read(root, "alpha", "theirs").is_ok() {
                saw.push("read");
            }
            if changes::read_all(root, "alpha")
                .records
                .iter()
                .any(|r| theirs(&r.change))
            {
                saw.push("read_all");
            }
        }
        other => panic!("no store {other}"),
    }
    saw
}

const STORES: [&str; 4] = ["todos", "memory", "sessions", "changes"];

#[test]
fn a_store_linked_to_another_workspace_or_a_persona_is_refused_and_nothing_is_written() {
    charter_core::unsteered!();
    for store in STORES {
        for target in ["../beta", "../../personas/devops"] {
            let p = project();
            std::os::unix::fs::symlink(
                format!("{target}/{store}"),
                p.path().join("workspaces/alpha").join(store),
            )
            .unwrap();
            let before = snapshot(p.path());

            let answered: Vec<_> = writers(&p, store)
                .into_iter()
                .filter(|(_, ok)| *ok)
                .map(|(writer, _)| writer)
                .collect();
            assert_eq!(
                answered,
                Vec::<&str>::new(),
                "{store} -> {target}: these wrote"
            );
            assert_eq!(
                snapshot(p.path()),
                before,
                "{store} -> {target}: something was written"
            );
            assert_eq!(
                readers_see_theirs(&p, store),
                Vec::<&str>::new(),
                "{store} -> {target}: these read through the link"
            );
        }
    }
}

#[test]
fn a_link_inside_a_store_to_another_workspace_is_never_followed() {
    charter_core::unsteered!();
    for store in STORES {
        let p = project();
        let mine = p.path().join("workspaces/alpha").join(store);
        std::fs::create_dir_all(&mine).unwrap();
        for name in [
            "MEMORY.md",
            "theirs.md",
            "20261003-080000-theirs.md",
            "theirs.json",
            "archive",
        ] {
            std::os::unix::fs::symlink(format!("../../beta/{store}/{name}"), mine.join(name))
                .unwrap();
        }
        let beta = p.path().join("workspaces/beta");
        let persona = p.path().join("personas");
        let before = (snapshot(&beta), snapshot(&persona));

        let _ = writers(&p, store);

        assert_eq!(
            (snapshot(&beta), snapshot(&persona)),
            before,
            "{store}: something outside alpha was written"
        );
        assert_eq!(
            readers_see_theirs(&p, store),
            Vec::<&str>::new(),
            "{store}: these read through a link"
        );
    }
}

#[test]
fn a_memory_is_never_archived_through_an_archive_linked_to_another_workspace() {
    charter_core::unsteered!();
    let p = project();
    let ws = alpha(&p);
    let mine = ws.remember("Mine to archive", at(9, 0)).unwrap();
    std::os::unix::fs::symlink("../../beta/memory/archive", ws.dir().join("memory/archive"))
        .unwrap();
    let beta = snapshot(&p.path().join("workspaces/beta"));

    let slug = mine.file_stem().unwrap().to_string_lossy().into_owned();
    assert!(ws.archive_memory(&slug).is_err());

    assert!(mine.is_file(), "the memory stays where it was");
    assert_eq!(snapshot(&p.path().join("workspaces/beta")), beta);
}

#[test]
fn a_memory_written_while_its_index_cannot_take_the_line_leaves_no_file_behind() {
    charter_core::unsteered!();
    // #1058: the memory file and its index line go together or not at all.
    use std::os::unix::fs::PermissionsExt;
    let p = project();
    let ws = alpha(&p);
    ws.remember("The first fact", at(9, 0)).unwrap();
    let memory = ws.dir().join("memory");
    let index = memory.join("MEMORY.md");
    std::fs::set_permissions(&index, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = snapshot(&memory);

    let refused = ws.remember("The second fact", at(9, 1));

    std::fs::set_permissions(&index, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        snapshot(&memory),
        before,
        "a memory file nothing indexes was left"
    );
}

/// `remember` in alpha after `plant` made its journal's index refuse the line: refused, and
/// alpha's journal exactly as it was (#1058).
fn refused_at_the_index(plant: impl FnOnce(&Path)) {
    let p = project();
    let ws = alpha(&p);
    ws.remember("The first fact", at(9, 0)).unwrap();
    let memory = ws.dir().join("memory");
    plant(&memory);
    let before = snapshot(&memory);

    let refused = ws.remember("The second fact", at(9, 1));

    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        snapshot(&memory),
        before,
        "a memory file nothing indexes was left"
    );
}

#[test]
fn a_journal_whose_index_is_a_link_inside_the_project_takes_no_memory() {
    charter_core::unsteered!();
    refused_at_the_index(|memory| {
        std::fs::remove_file(memory.join("MEMORY.md")).unwrap();
        std::os::unix::fs::symlink("../../beta/memory/MEMORY.md", memory.join("MEMORY.md"))
            .unwrap();
    });
}

#[test]
fn a_journal_whose_index_is_a_fifo_takes_no_memory_and_answers_at_once() {
    charter_core::unsteered!();
    refused_at_the_index(|memory| {
        std::fs::remove_file(memory.join("MEMORY.md")).unwrap();
        let made = charter_core::forklock::status(
            std::process::Command::new("mkfifo").arg(memory.join("MEMORY.md")),
        )
        .expect("mkfifo runs");
        assert!(made.success());
    });
}

#[test]
fn a_journal_whose_index_is_a_fifo_is_refused_as_not_a_file_rather_than_a_link() {
    charter_core::unsteered!();
    let p = project();
    let ws = alpha(&p);
    ws.remember("The first fact", at(9, 0)).unwrap();
    let index = ws.dir().join("memory/MEMORY.md");
    std::fs::remove_file(&index).unwrap();
    let made = charter_core::forklock::status(std::process::Command::new("mkfifo").arg(&index))
        .expect("mkfifo runs");
    assert!(made.success());

    let why = ws
        .remember("The second fact", at(9, 1))
        .unwrap_err()
        .to_string();

    assert!(
        why.contains("neither a file nor a directory") && !why.contains("through a link"),
        "{why}"
    );
}

#[test]
fn a_store_named_by_a_path_that_walks_out_of_its_workspace_is_refused_not_followed() {
    charter_core::unsteered!();
    // Whoever builds the path, one below `workspaces/` that is not `<ws>/<store>/…` is refused
    // rather than written by path, so the hold never rests on how a caller spells it.
    let p = project();
    let persona = p.path().join("personas");
    let before = snapshot(&persona);
    let walked = p
        .path()
        .join("workspaces/alpha/../../personas/devops/memory");

    let wrote = charter_core::memstore::write(
        p.path(),
        &walked,
        "Planted by a walk",
        None,
        true,
        "persistent",
        true,
        at(9, 0),
    );

    assert!(wrote.is_err(), "{wrote:?}");
    assert_eq!(snapshot(&persona), before);
    let (found, _) = charter_core::memstore::read_entries(p.path(), &walked);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_store_a_chat_keeps_locked_refuses_the_operators_write_in_bounded_time() {
    charter_core::unsteered!();
    // A chat can flock its own store and never let go; the operator's command must answer.
    let p = project();
    let ws = alpha(&p);
    ws.remember("The first fact", at(9, 0)).unwrap();
    let memory = ws.dir().join("memory");
    let held = std::fs::File::open(&memory).unwrap();
    rustix::fs::flock(&held, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let before = snapshot(&memory);

    let (tx, rx) = std::sync::mpsc::channel();
    let root = p.path().to_path_buf();
    std::thread::spawn(move || {
        let ws = Plane::open(&root).workspace("alpha").unwrap();
        let _ = tx.send(
            ws.remember("The second fact", at(9, 1))
                .map_err(|e| e.to_string()),
        );
    });
    let answered = rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("answered, not wedged");

    let why = answered.expect_err("refused while the store is locked");
    assert!(why.contains("busy"), "{why}");
    assert_eq!(snapshot(&memory), before);
    drop(held);
}

/// The race HP-7 measured for the MCP tools, run against the operator's own write: a thread
/// swaps alpha's memory directory with a link to a persona's memory as fast as it can while
/// `remember` writes. Everything a write answered for is inside alpha, and nothing outside it
/// changes.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_store_swapped_for_a_link_while_remember_runs_never_carries_a_write_outside() {
    charter_core::unsteered!();
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const CALLS: usize = 1_500;
    let p = project();
    let ws = alpha(&p);
    let alpha_dir = ws.dir().to_path_buf();
    std::fs::create_dir_all(alpha_dir.join("memory")).unwrap();
    std::os::unix::fs::symlink("../../personas/devops/memory", alpha_dir.join("swap")).unwrap();
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
        let (from, to) = (alpha_dir.join("memory"), alpha_dir.join("swap"));
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
        if ws.remember(&format!("Raced note {i}"), at(9, 0)).is_ok() {
            written += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();

    assert!(swaps.load(Ordering::Relaxed) > 0, "the racer never swapped");
    let after = outside();
    assert!(
        after == before,
        "something was written or changed outside alpha"
    );
    let inside = snapshot(&alpha_dir)
        .iter()
        .filter(|(path, _)| path.to_string_lossy().contains("raced-note"))
        .count();
    assert_eq!(inside, written, "{written} answered, {inside} inside alpha");
}
