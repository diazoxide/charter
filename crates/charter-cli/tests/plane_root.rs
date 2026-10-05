//! A chat the app started at the plane root (`$CHARTER_PLANE_ROOT_SESSION=1`, SI-1), asked of
//! the binary: it is in no workspace, so a command that needs one refuses and says to pass
//! `-w` rather than act on a workspace charter picked for it; and a command that only reads a
//! workspace when there is one still runs.
//!
//! A subprocess for `active.rs`'s reason: the rung is an environment variable.

use std::path::{Path, PathBuf};
use std::process::Command;

fn charter() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_charter"))
}

/// A plane with `alpha` and `beta`, and `[workspace] default = "alpha"`, so a command that
/// fell through the ladder would land somewhere a test can see.
fn plane(at: PathBuf) -> PathBuf {
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(
        at.join("charter.toml"),
        "schema = 1\n\n[workspace]\ndefault = \"alpha\"\n",
    )
    .unwrap();
    for name in ["alpha", "beta"] {
        std::fs::create_dir_all(at.join("workspaces").join(name)).unwrap();
    }
    at
}

struct Ran {
    code: i32,
    out: String,
    err: String,
}

/// `charter` in `cwd`, on the plane at `root`, with every identity variable cleared and then
/// `env` set.
fn run_in(root: &Path, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Ran {
    let mut command = Command::new(charter());
    // The machine store beside the plane, never the operator's: a fenced build refuses that.
    let store = root
        .parent()
        .expect("the plane has a parent")
        .join("config");
    command
        .args(args)
        .current_dir(cwd)
        .env("CHARTER_ROOT", root)
        .env("CHARTER_CONFIG_HOME", store);
    for name in [
        "CHARTER_WORKSPACE",
        "CHARTER_PLANE_ROOT_SESSION",
        "CHARTER_PERSONA",
        "CHARTER_SESSION_ID",
        "CLAUDE_CODE_SESSION_ID",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
        // The app's socket and chat number, which a suite run inside a chat the app started
        // would otherwise inherit — and `statusline` then draws nothing, the app's footer.
        charter_core::hookwire::SOCKET_ENV,
        charter_core::hookwire::CHAT_ENV,
    ] {
        // Under either name (V93k): a suite run in a chat inherits both.
        for spelling in charter_core::envvar::spellings(name) {
            command.env_remove(spelling);
        }
    }
    for (name, value) in env {
        command.env(name, value);
    }
    let out = command.output().expect("the binary runs");
    Ran {
        code: out.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&out.stdout).to_string(),
        err: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

const AT_ROOT: &[(&str, &str)] = &[
    ("CHARTER_PLANE_ROOT_SESSION", "1"),
    ("CHARTER_SESSION_ID", "7"),
];

fn at_root(root: &Path, args: &[&str]) -> Ran {
    run_in(root, root, args, AT_ROOT)
}

/// The sentence every refusal at the root opens with.
fn refused_at_root(ran: &Ran) -> bool {
    ran.code != 0
        && ran.err.contains("started at the plane root")
        && ran.err.contains("-w <workspace>")
        && ran.err.contains("`alpha`, `beta`")
}

#[test]
fn a_command_that_needs_a_workspace_refuses_at_the_root_and_says_to_pass_w() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));

    for args in [
        vec!["ws", "todo", "Cut the release"],
        vec!["ws", "todo"],
        vec!["workspace", "remember", "A fact"],
        vec!["workspace", "vision", "Ship it"],
        vec!["workspace", "recall"],
    ] {
        let ran = at_root(&root, &args);
        assert!(
            refused_at_root(&ran),
            "`charter {args:?}`: {} / {}",
            ran.code,
            ran.err
        );
    }
    // Nothing landed in the plane's default, which is where the ladder would have put it.
    assert!(!root.join("workspaces/alpha/todos").exists());
    assert!(!root.join("workspaces/alpha/memory").exists());
}

#[test]
fn naming_the_workspace_is_how_a_root_chat_manages_one() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));

    let ran = at_root(&root, &["ws", "todo", "-w", "beta", "Cut the release"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(root.join("workspaces/beta/todos").is_dir());

    // And `CHARTER_WORKSPACE=<name>` set on one command is the same request.
    let mut env = AT_ROOT.to_vec();
    env.push(("CHARTER_WORKSPACE", "beta"));
    let ran = run_in(&root, &root, &["workspace", "current"], &env);
    assert_eq!((ran.code, ran.out.as_str()), (0, "beta\n"), "{}", ran.err);
}

#[test]
fn the_current_workspace_of_a_root_chat_is_none_and_says_why() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));

    let ran = at_root(&root, &["workspace", "current"]);
    assert!(
        refused_at_root(&ran),
        "{} / {} / {}",
        ran.code,
        ran.out,
        ran.err
    );
    assert_eq!(ran.out, "", "no name a script could take for the answer");
}

#[test]
fn a_root_chat_is_not_moved_into_a_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));

    let ran = at_root(&root, &["workspace", "use", "beta"]);
    assert!(ran.code != 0, "{}", ran.err);
    assert!(ran.err.contains("plane root"), "{}", ran.err);
    assert!(!root.join(".charter/sessions/7.workspace").exists());

    // Creating one is managing the plane; selecting it is moving the chat, which is refused
    // before anything is made.
    let ran = at_root(&root, &["workspace", "create", "gamma", "--use"]);
    assert!(ran.code != 0, "{}", ran.err);
    assert!(ran.err.contains("--use"), "{}", ran.err);
    assert!(!root.join("workspaces/gamma").exists());
    let ran = at_root(&root, &["workspace", "create", "gamma"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(root.join("workspaces/gamma").is_dir());
}

#[test]
fn recall_at_the_root_searches_everything_but_a_workspace() {
    // The call a harness makes at session start, with no flags. It must run.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));
    let ran = run_in(
        &root,
        &root,
        &["workspace", "remember", "-w", "alpha", "Alpha's own fact"],
        &[],
    );
    assert_eq!(ran.code, 0, "{}", ran.err);

    let ran = at_root(&root, &["recall", "Alpha"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(!ran.out.contains("Alpha's own fact"), "{}", ran.out);
    let ran = at_root(&root, &["recall", "-w", "alpha", "Alpha"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("Alpha's own fact"), "{}", ran.out);
}

#[test]
fn standing_in_a_workspaces_own_directory_is_being_in_it() {
    // The ladder and the window agree on `workspaces/<ws>` (SI-1).
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));

    let ran = run_in(
        &root,
        &root.join("workspaces/beta"),
        &["workspace", "current"],
        &[],
    );
    assert_eq!((ran.code, ran.out.as_str()), (0, "beta\n"), "{}", ran.err);
}

// ---- SI-1b: standing anywhere in the plane outside every workspace ------------------------

#[test]
fn a_session_standing_in_the_plane_outside_every_workspace_is_at_the_plane_root() {
    // No launcher mark, no pointer, no `-w`: the plane's default (`alpha`) no longer speaks
    // for a session that is standing somewhere in the plane.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));
    std::fs::create_dir_all(root.join("docs")).unwrap();

    for cwd in [root.clone(), root.join("docs")] {
        let ran = run_in(&root, &cwd, &["ws", "todo", "Cut the release"], &[]);
        assert!(
            ran.code != 0 && ran.err.contains("at the plane root") && ran.err.contains("-w"),
            "{cwd:?}: {} / {}",
            ran.code,
            ran.err
        );
        let ran = run_in(&root, &cwd, &["workspace", "current"], &[]);
        assert!(ran.code != 0 && ran.out.is_empty(), "{cwd:?}: {}", ran.out);
    }
    assert!(!root.join("workspaces/alpha/todos").exists());
}

#[test]
fn a_session_standing_at_the_root_moves_into_a_workspace_with_workspace_use() {
    // Nothing pinned it to the root, so `workspace use` works as it always has, and from then
    // on its own pointer answers.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));
    let session = &[("CHARTER_SESSION_ID", "7")];

    let ran = run_in(&root, &root, &["workspace", "use", "beta"], session);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let ran = run_in(&root, &root, &["workspace", "current"], session);
    assert_eq!((ran.code, ran.out.as_str()), (0, "beta\n"), "{}", ran.err);
}

#[test]
fn the_planes_default_still_answers_for_a_caller_outside_the_plane() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();

    let ran = run_in(&root, &elsewhere, &["workspace", "current"], &[]);
    assert_eq!((ran.code, ran.out.as_str()), (0, "alpha\n"), "{}", ran.err);
}

#[test]
fn the_footer_of_a_root_chat_shows_the_plane_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"));
    let ran = run_in(
        &root,
        &root,
        &["statusline", "--now", "2026-05-04T11:32:17"],
        AT_ROOT,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("plane root"), "{}", ran.out);
    assert!(!ran.out.contains("alpha"), "{}", ran.out);
}
