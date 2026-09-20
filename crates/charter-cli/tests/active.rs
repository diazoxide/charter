//! The active workspace and the active persona, asked of the binary itself.
//!
//! A subprocess and not a unit test, for `plane_resolution.rs`'s reason: most of these rungs
//! are environment variables, and setting one in-process is `unsafe` under this edition —
//! which the workspace forbids — and would leak into every other test in the binary.
//!
//! The ladders themselves are unit-tested rung by rung in `charter_core::active`, and proved
//! against the Python charter scenario by scenario in `tests/differential/run.py`. What is
//! here is the part neither of those can see: that the COMMANDS reach the ladder at all.

use std::path::{Path, PathBuf};
use std::process::Command;

fn charter() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_charter"))
}

/// A plane with one workspace directory per name given.
fn plane(at: PathBuf, workspaces: &[&str]) -> PathBuf {
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(at.join("charter.toml"), "schema = 1\n").unwrap();
    for name in workspaces {
        std::fs::create_dir_all(at.join("workspaces").join(name)).unwrap();
    }
    at
}

struct Ran {
    code: i32,
    out: String,
    err: String,
}

/// Run `charter` at the plane root.
fn run(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Ran {
    run_in(root, root, args, env)
}

/// Run `charter` in `cwd`, with `root` pinned as the plane and every identity variable
/// cleared, so a suite running inside a real chat does not inherit that chat's session, pane
/// or workspace.
///
/// The two paths are separate because the tree you are standing in IS a rung, and pinning the
/// plane to the working directory would make that rung untestable.
fn run_in(root: &Path, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Ran {
    let mut command = Command::new(charter());
    command
        .args(args)
        .current_dir(cwd)
        .env("CHARTER_ROOT", root);
    for name in [
        "CHARTER_WORKSPACE",
        "CHARTER_PERSONA",
        "CHARTER_SESSION_ID",
        "CLAUDE_CODE_SESSION_ID",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
    ] {
        command.env_remove(name);
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

#[test]
fn a_write_with_no_w_lands_in_the_workspace_the_environment_names() {
    // **The whole of M2.9 at one command.** Before it, `-w` was required, so this exited 2 —
    // and 2 is the code a harness reads as "block".
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha", "beta"]);

    let ran = run(
        &root,
        &["workspace", "remember", "The importer drops rows over 4 MB"],
        &[("CHARTER_WORKSPACE", "beta")],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    let written: Vec<String> = std::fs::read_dir(root.join("workspaces/beta/memory"))
        .expect("beta has a memory store")
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|name| name != "MEMORY.md")
        .collect();
    assert_eq!(written.len(), 1, "one memory in beta, found {written:?}");
    assert!(
        !root.join("workspaces/alpha/memory").exists(),
        "the memory landed in alpha as well as beta"
    );
}

#[test]
fn no_command_that_takes_w_still_refuses_to_run_without_one() {
    // Every command that took a required `-w`, in one test, because the defect was the set
    // and not any one of them: a flag declared `required` is a command a harness cannot call.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha"]);

    for args in [
        vec!["workspace", "vision", "Ship it"],
        vec!["workspace", "remember", "A fact"],
        vec!["workspace", "recall"],
        vec!["workspace", "note", "A note"],
        vec!["ws", "todo", "A todo"],
        vec!["ws", "todo"],
    ] {
        let ran = run(&root, &args, &[("CHARTER_WORKSPACE", "alpha")]);
        assert_eq!(ran.code, 0, "`charter {args:?}` failed: {}", ran.err);
    }
}

#[test]
fn recall_with_no_flags_at_all_is_the_call_a_harness_makes() {
    // **The command M2.9 exists for.** It refused with exit 2 — "recall needs -w <workspace>
    // … this charter does not resolve the active workspace" — and a harness calls it exactly
    // like this at session start, with nothing to pass.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha"]);

    let ran = run(&root, &["recall"], &[]);

    assert_eq!(ran.code, 0, "{}", ran.err);
}

#[test]
fn recall_searches_the_workspace_the_environment_names() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha", "beta"]);
    let memory = root.join("workspaces/beta/memory");
    std::fs::create_dir_all(&memory).unwrap();
    std::fs::write(
        memory.join("20260302-091200-the-importer-drops-rows.md"),
        "# The importer drops rows\n\n_2026-03-02 09:12 · persistent_\n\nOver 4 MB.\n",
    )
    .unwrap();

    let found = run(&root, &["recall"], &[("CHARTER_WORKSPACE", "beta")]);
    let not = run(&root, &["recall"], &[("CHARTER_WORKSPACE", "alpha")]);

    assert!(
        found.out.contains("The importer drops rows"),
        "beta's journal was not searched: {:?} / {:?}",
        found.out,
        found.err
    );
    assert!(
        !not.out.contains("The importer drops rows"),
        "alpha's recall found beta's memory: {:?}",
        not.out
    );
}

#[test]
fn the_flag_still_outranks_the_environment() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha", "beta"]);

    let ran = run(
        &root,
        &["workspace", "vision", "Ship it", "-w", "alpha"],
        &[("CHARTER_WORKSPACE", "beta")],
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(root.join("workspaces/alpha/workspace.md").is_file());
    assert!(!root.join("workspaces/beta/workspace.md").exists());
}

#[test]
fn current_prints_the_resolved_workspace_and_nothing_else() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha"]);

    let ran = run(&root, &["workspace", "current"], &[]);
    assert_eq!(ran.out, "default\n");
    // Nothing on stderr: charter explains the rung there and this port does not, and a
    // scenario in the differential run records that as the difference it is.
    assert_eq!(ran.err, "");
    assert_eq!(
        run(
            &root,
            &["workspace", "current"],
            &[("CHARTER_WORKSPACE", "alpha")]
        )
        .out,
        "alpha\n"
    );
}

#[test]
fn current_reads_the_session_pointer_the_python_charter_wrote() {
    // The rung a harness actually lands on: `charter workspace use` writes this file, and
    // during the migration the charter that wrote it is the Python one.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha"]);
    std::fs::create_dir_all(root.join(".charter/sessions")).unwrap();
    std::fs::write(root.join(".charter/sessions/s1.workspace"), "alpha\n").unwrap();

    let ran = run(
        &root,
        &["workspace", "current"],
        &[("CHARTER_SESSION_ID", "s1")],
    );

    assert_eq!(ran.out, "alpha\n");
}

#[test]
fn the_tree_you_are_standing_in_outranks_the_session_pointer() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha", "beta"]);
    std::fs::create_dir_all(root.join(".charter/sessions")).unwrap();
    std::fs::write(root.join(".charter/sessions/s1.workspace"), "alpha\n").unwrap();
    let tree = root.join("workspaces/beta/svc");
    std::fs::create_dir_all(&tree).unwrap();

    let ran = run_in(
        &root,
        &tree,
        &["workspace", "current"],
        &[("CHARTER_SESSION_ID", "s1")],
    );

    assert_eq!(ran.out, "beta\n");
}

#[test]
fn the_tmux_frames_launch_record_is_not_a_rung_here() {
    // **The one rung this port deliberately does not have**, pinned from THIS side.
    // `docs/plane-format.md` rules that `.charter/frame/**` is the tmux frame's and that the
    // app neither reads nor writes there, so nothing here can ever set it.
    //
    // The differential scenario says only that Python answers something ELSE; that is an
    // inequality, and an inequality is satisfied by any wrong answer. This says which answer
    // this binary gives: the rung below, exactly.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["from-frame", "from-terminal"]);
    std::fs::create_dir_all(root.join(".charter/frame/s1")).unwrap();
    std::fs::write(root.join(".charter/frame/s1/workspace"), "from-frame\n").unwrap();
    std::fs::create_dir_all(root.join(".charter/terminals")).unwrap();
    std::fs::write(
        root.join(".charter/terminals/pane-1.workspace"),
        "from-terminal\n",
    )
    .unwrap();

    let ran = run(
        &root,
        &["workspace", "current"],
        &[("CHARTER_SESSION_ID", "s1"), ("TERM_SESSION_ID", "pane-1")],
    );

    assert_eq!(ran.out, "from-terminal\n");
}

#[test]
fn a_plane_with_no_front_door_reports_no_persona() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &[]);

    assert_eq!(run(&root, &["persona", "current"], &[]).out, "(none)\n");
}

#[test]
fn the_persona_a_chat_was_launched_with_is_the_one_reported() {
    // `start::environment` puts `CHARTER_PERSONA` in every chat the app starts, and this is
    // the rung that reads it back — the one thing tying what the app launched to what a
    // command inside that chat acts as.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &[]);

    let ran = run(
        &root,
        &["persona", "current"],
        &[("CHARTER_PERSONA", "devops")],
    );

    assert_eq!(ran.out, "devops\n");
}

#[test]
fn the_planes_declared_front_door_is_reported_when_it_is_one_the_plane_has() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &[]);
    std::fs::write(
        root.join("charter.toml"),
        "schema = 1\n\n[persona]\ndefault = \"steward\"\n",
    )
    .unwrap();

    // Declared and absent: nothing stands in for it, and `(none)` is the honest answer.
    assert_eq!(run(&root, &["persona", "current"], &[]).out, "(none)\n");

    std::fs::create_dir_all(root.join("personas/steward")).unwrap();
    std::fs::write(root.join("personas/steward/persona.md"), "# steward\n").unwrap();
    assert_eq!(run(&root, &["persona", "current"], &[]).out, "steward\n");
}

#[test]
fn standing_in_a_clone_that_is_itself_a_plane_the_binary_acts_on_the_plane_holding_it() {
    // `charter.toml` is tracked, so a clone of a plane carries one and `charter clone` puts
    // clones at `workspaces/<ws>/<repo>`. `charter root` stopped at the inner one while
    // `charter init` — the same ladder, a different entry point — answered the outer.
    let tmp = tempfile::tempdir().unwrap();
    let outer = plane(tmp.path().join("outer"), &["ide"]);
    let inner = plane(outer.join("workspaces/ide/charter"), &[]);
    let deep = inner.join("crates/src");
    std::fs::create_dir_all(&deep).unwrap();

    // `CHARTER_ROOT` is the escape hatch for anyone who genuinely means the inner plane, so
    // it must be out of the way for the walk to be what answers.
    let out = Command::new(charter())
        .arg("root")
        .current_dir(&deep)
        .env_remove("CHARTER_ROOT")
        .output()
        .expect("the binary runs");

    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        outer.canonicalize().unwrap().display().to_string()
    );
}
