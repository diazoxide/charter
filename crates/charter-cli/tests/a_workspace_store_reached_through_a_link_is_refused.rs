//! V74 for the operator's own commands (#1064): every `charter` command that writes a
//! workspace's todos, memory, session records or changes refuses a store a chat linked to
//! another workspace or a persona, even though the link stays inside the project, and writes
//! nothing anywhere. The commands run outside a chat's sandbox, so a link a chat leaves in its
//! own workspace must not carry the operator's next write somewhere else.
#![cfg(unix)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A copy of the committed `daily` fixture plane, with `beta` and the persona `devops` each
/// holding a file of every kind each store keeps.
fn daily() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().unwrap();
    copy(&fixture, &dir.path().join("plane"));
    let root = root(&dir);
    for target in ["workspaces/beta", "personas/devops"] {
        for store in ["todos", "memory", "sessions", "changes"] {
            let store = root.join(target).join(store);
            std::fs::create_dir_all(store.join("archive")).unwrap();
            std::fs::write(
                store.join("MEMORY.md"),
                "# Theirs\n\n- [Theirs](theirs.md)\n",
            )
            .unwrap();
            std::fs::write(
                store.join("theirs.md"),
                "# Theirs\n\n_2026-10-01 09:00 · persistent_\n\nTheirs.\n",
            )
            .unwrap();
            std::fs::write(store.join("archive/gone.md"), "# Gone\n\nGone.\n").unwrap();
            std::fs::write(
                store.join("theirs.json"),
                r#"{"change": "theirs", "why": "theirs", "created": "2026-10-01", "by": "them", "members": [], "excluded": []}"#,
            )
            .unwrap();
        }
    }
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

/// `charter <args>` in the plane with nothing of the caller's environment but what is named.
fn charter(tmp: &tempfile::TempDir, args: &[&str], stdin: &str) -> Output {
    let root = root(tmp);
    let mut child = Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(args)
        .current_dir(&root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("CHARTER_ROOT", &root)
        .env("HOME", tmp.path().join("home"))
        .env("NO_COLOR", "1")
        .env("CHARTER_NO_BACKGROUND_CHECKS", "1")
        .env("CHARTER_WORKSPACE", "alpha")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
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

const BODY: &str = "## Goal\n\nG.\n\n## Done\n\n- d\n\n## Decisions\n\n- x\n\n## Open\n\nNothing.\n\n\
## How to resume\n\nRead it.\n";

/// Every command that writes `store`, as `(args, stdin)`.
fn writers(store: &str) -> Vec<(Vec<&'static str>, &'static str)> {
    let w = |args: &[&'static str]| {
        let mut all = args.to_vec();
        all.extend(["-w", "alpha"]);
        (all, "")
    };
    match store {
        "todos" => vec![
            w(&["ws", "todo", "Planted through a link"]),
            w(&["ws", "todo", "done", "theirs"]),
            w(&["ws", "todo", "forget", "theirs"]),
        ],
        "memory" => vec![
            w(&[
                "workspace",
                "remember",
                "Planted through a link",
                "--no-sync",
            ]),
            w(&["workspace", "forget", "theirs"]),
            w(&[
                "workspace",
                "edit",
                "theirs",
                "--title",
                "Mine now",
                "Mine.",
            ]),
            w(&["workspace", "archive", "theirs"]),
            w(&["workspace", "unarchive", "gone"]),
            w(&["workspace", "move-memory", "theirs", "--to-shared"]),
        ],
        "sessions" => vec![(
            vec![
                "session",
                "record",
                "--title",
                "Planted",
                "--now",
                "2026-10-03T09:00:00",
            ],
            BODY,
        )],
        "changes" => vec![
            w(&["change", "create", "planted", "--why", "planted"]),
            w(&["change", "forget", "theirs"]),
        ],
        other => panic!("no store {other}"),
    }
}

#[test]
fn every_writer_refuses_a_store_linked_to_another_workspace_or_a_persona() {
    for store in ["todos", "memory", "sessions", "changes"] {
        for target in ["../beta", "../../personas/devops"] {
            for (args, stdin) in writers(store) {
                let tmp = daily();
                let mine = root(&tmp).join("workspaces/alpha").join(store);
                let _ = std::fs::remove_dir_all(&mine);
                std::os::unix::fs::symlink(format!("{target}/{store}"), &mine).unwrap();
                let before = snapshot(&root(&tmp));

                let out = charter(&tmp, &args, stdin);

                assert_ne!(
                    out.status.code(),
                    Some(0),
                    "{args:?} through {store} -> {target} answered 0: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let after = snapshot(&root(&tmp));
                let changed: Vec<&PathBuf> = after
                    .iter()
                    .filter(|entry| !before.contains(entry))
                    .chain(before.iter().filter(|entry| !after.contains(entry)))
                    .map(|(path, _)| path)
                    .filter(|path| !path.starts_with(root(&tmp).join(".charter")))
                    .collect();
                assert!(
                    changed.is_empty(),
                    "{args:?} through {store} -> {target} changed {changed:?}"
                );
            }
        }
    }
}
