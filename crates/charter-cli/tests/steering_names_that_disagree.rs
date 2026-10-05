//! D-RN2d-8: a variable that chooses what a command acts on (`ROOT`, `HOME`, `WORKSPACE`, …),
//! set under both its names with two different values, is refused rather than silently read
//! under one of them. Inside a chat both names carry the same value, so an inline
//! `CHARTER_ROOT=… charter save` is the case this catches: it would otherwise act on the
//! chat's own project.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn plane(at: PathBuf) -> PathBuf {
    std::fs::create_dir_all(at.join("workspaces/alpha")).unwrap();
    std::fs::write(
        at.join("charter.toml"),
        "schema = 1\n\n[workspace]\ndefault = \"alpha\"\n",
    )
    .unwrap();
    at
}

struct Ran {
    code: i32,
    out: String,
    err: String,
}

/// `charter args…` with none of the product's variables but `env`, and `stdin` on its input.
fn charter(cwd: &Path, args: &[&str], env: &[(&str, &Path)], stdin: &str) -> Ran {
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command.args(args).current_dir(cwd);
    for (name, _) in std::env::vars_os() {
        if let Some(name) = name.to_str()
            && charter_core::envvar::rest(name).is_some()
            && name != charter_core::fence::VAR
        {
            command.env_remove(name);
        }
    }
    command
        .env("PURLIS_CONFIG_HOME", cwd.join("config"))
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("the binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Ran {
        code: out.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

const SAID: &str = "PURLIS_ROOT and CHARTER_ROOT disagree — set both to the same value \
                    (PURLIS_ROOT=<x> CHARTER_ROOT=<x> charter …); CHARTER_ROOT is the old name.";

#[test]
fn a_root_named_twice_with_two_values_is_refused_naming_both() {
    let tmp = tempfile::tempdir().unwrap();
    let chats = plane(tmp.path().join("chats"));
    let other = plane(tmp.path().join("other"));

    let ran = charter(
        tmp.path(),
        &["workspace", "current"],
        &[("PURLIS_ROOT", &chats), ("CHARTER_ROOT", &other)],
        "",
    );

    assert_ne!(ran.code, 0, "{}", ran.out);
    assert!(ran.err.contains(SAID), "{}", ran.err);
    assert_eq!(ran.out, "");
}

#[test]
fn equal_twins_and_the_old_name_alone_are_read() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));

    for env in [
        vec![
            ("PURLIS_ROOT", root.as_path()),
            ("CHARTER_ROOT", root.as_path()),
        ],
        vec![("CHARTER_ROOT", root.as_path())],
    ] {
        let ran = charter(tmp.path(), &["workspace", "current"], &env, "");
        assert_eq!((ran.code, ran.out.as_str()), (0, "alpha\n"), "{}", ran.err);
    }
}

#[test]
fn a_tool_hook_blocks_and_a_state_hook_stands_aside() {
    let tmp = tempfile::tempdir().unwrap();
    let chats = plane(tmp.path().join("chats"));
    let other = plane(tmp.path().join("other"));
    let env = [
        ("PURLIS_ROOT", chats.as_path()),
        ("CHARTER_ROOT", other.as_path()),
    ];
    let payload = r#"{"tool_name":"Bash","tool_input":{"command":"true"}}"#;

    // A tool call is refused: the guard would otherwise judge it against a project it guessed.
    let guard = charter(tmp.path(), &["hook", "pretooluse"], &env, payload);
    assert_eq!(guard.code, 2, "{}", guard.err);
    assert!(guard.err.contains(SAID), "{}", guard.err);

    // A hook that only reports or briefs acts on nothing, and never wedges the chat.
    let stop = charter(tmp.path(), &["hook", "stop"], &env, "{}");
    assert_eq!(stop.code, 0, "{}", stop.err);
    assert!(stop.err.contains(SAID), "{}", stop.err);
    assert_eq!(stop.out, "");
}

/// Inside a chat both names carry the chat's project. One name typed in front of a command is
/// refused, with a remedy that works there: both names, set to the same value.
#[test]
fn in_a_chat_one_name_typed_inline_is_refused_and_both_names_are_obeyed() {
    let tmp = tempfile::tempdir().unwrap();
    let chats = plane(tmp.path().join("chats"));
    let other = tmp.path().join("other");
    plane(other.clone());
    std::fs::create_dir_all(other.join("workspaces/beta")).unwrap();
    std::fs::write(
        other.join("charter.toml"),
        "schema = 1\n\n[workspace]\ndefault = \"beta\"\n",
    )
    .unwrap();

    // The chat's twins, then `PURLIS_ROOT=<other>` typed in front of the command.
    let inline = charter(
        tmp.path(),
        &["workspace", "current"],
        &[("CHARTER_ROOT", &chats), ("PURLIS_ROOT", &other)],
        "",
    );
    assert_ne!(inline.code, 0, "{}", inline.out);
    assert!(inline.err.contains(SAID), "{}", inline.err);

    // The remedy the sentence names: both, to the same value, act on the other project.
    let both = charter(
        tmp.path(),
        &["workspace", "current"],
        &[("CHARTER_ROOT", &other), ("PURLIS_ROOT", &other)],
        "",
    );
    assert_eq!(
        (both.code, both.out.as_str()),
        (0, "beta\n"),
        "{}",
        both.err
    );
}

/// A commit in a repository that is no project needs none of the project's variables, so a
/// disagreement between them never blocks it.
#[test]
fn a_commit_in_a_repository_that_is_no_project_is_never_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(repo.join("a.txt"), "hello\n").unwrap();
    git(&["add", "a.txt"]);
    std::fs::write(repo.join("MSG"), "a message\n").unwrap();
    let a = plane(tmp.path().join("a"));
    let b = plane(tmp.path().join("b"));
    let env = [("PURLIS_ROOT", a.as_path()), ("CHARTER_ROOT", b.as_path())];

    for args in [
        &["git-hook", "pre-commit"][..],
        &["git-hook", "commit-msg", "MSG"],
    ] {
        let ran = charter(&repo, args, &env, "");
        assert_eq!(ran.code, 0, "{args:?}: {}", ran.err);
        assert!(!ran.err.contains("disagree"), "{args:?}: {}", ran.err);
    }
}
