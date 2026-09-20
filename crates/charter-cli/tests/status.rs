//! `charter status` and `charter docs generate`, asked of the binary itself.
//!
//! A subprocess and not a unit test, for `active.rs`'s reason: the rungs `status` reports on
//! are environment variables, and setting one in-process is `unsafe` under this edition.
//!
//! What the differential harness cannot see is here: the nested-plane notice names two
//! ABSOLUTE paths, which are two different directories on the two sides of that comparison,
//! so it can only be asserted where one plane is built and one binary runs.

use std::path::{Path, PathBuf};
use std::process::Command;

fn charter() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_charter"))
}

struct Ran {
    code: i32,
    out: String,
    err: String,
}

fn run_in(root: &Path, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Ran {
    let mut command = Command::new(charter());
    command
        .args(args)
        .current_dir(cwd)
        .env("CHARTER_ROOT", root)
        .env("NO_COLOR", "1");
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

fn run(root: &Path, args: &[&str]) -> Ran {
    run_in(root, root, args, &[])
}

/// A plane declaring `acme` on GitHub, with one workspace directory per name given.
fn plane(at: PathBuf, workspaces: &[&str]) -> PathBuf {
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(
        at.join("charter.toml"),
        "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n",
    )
    .unwrap();
    for name in workspaces {
        std::fs::create_dir_all(at.join("workspaces").join(name)).unwrap();
    }
    at
}

fn inventory(root: &Path, body: &str) {
    std::fs::create_dir_all(root.join("inventory")).unwrap();
    std::fs::write(root.join("inventory/repos.json"), body).unwrap();
}

#[test]
fn status_says_which_plane_answered_and_why_the_workspace_was_chosen() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha", "beta"]);
    let ran = run(&root, &["status", "-w", "beta"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    let lines: Vec<&str> = ran.out.lines().collect();
    assert_eq!(
        lines[0],
        "acme: 0 repos in inventory · 2 workspace(s) · active: beta (via --workspace)"
    );
    assert_eq!(lines[1], "");
    assert_eq!(lines[2], "    alpha  (0 cloned)");
    assert_eq!(lines[3], "  * beta  (0 cloned)");
    assert!(
        ran.out.contains("— workspace: beta (active) · 0 repo(s) —"),
        "{}",
        ran.out
    );
    assert!(
        ran.out
            .contains("  (empty; `charter clone <repo> --workspace beta` to populate)"),
        "{}",
        ran.out
    );
    assert!(
        ran.out
            .ends_with("(0 repos available to clone — see docs/topology.md)\n"),
        "{}",
        ran.out
    );
}

#[test]
fn a_shell_with_no_pane_id_is_told_that_is_why_it_is_on_default() {
    // Not "default (nothing selected)": with no pane there is no terminal pointer to fall
    // back on, so every session in this shell starts on `default` however many times the
    // operator picks — and a surface that asserts the answer with no reason is the one the
    // operator asked "why am I in default workspace again?" of.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &[]);
    let ran = run(&root, &["status"]);

    assert!(
        ran.out.starts_with(
            "acme: 0 repos in inventory · 0 workspace(s) · active: default \
             (via default (no pane id — nothing persists between sessions))\n"
        ),
        "{}",
        ran.out
    );
    // No workspaces at all: the roster block is skipped and the section follows the header.
    assert!(
        ran.out.contains("\n\n— workspace: default (active)"),
        "{:?}",
        ran.out
    );
    // And nothing is refused. The ladder always ends on a name, so this plane has an active
    // workspace with no directory behind it — an empty workspace, not a containment
    // failure. `confine::workspace_dir` cannot tell those apart, and saying "not a directory
    // charter can resolve" about a plane nobody has cloned into yet is noise on the one
    // command an operator runs when they do not know where they are.
    assert_eq!(ran.err, "", "an uncreated workspace is not a refusal");
}

#[test]
fn standing_in_a_clone_that_is_itself_a_plane_says_which_one_answered() {
    // `charter.toml` is tracked, so every clone of a plane is a plane, and `charter clone`
    // puts clones exactly where the upward walk meets them first (charter #200). The notice
    // names both by PATH: clone charter into its own plane and both are called `charter`.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha"]);
    let inner = root.join("workspaces/alpha/tool");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("charter.toml"), "schema = 1\n").unwrap();

    let ran = run_in(&root, &inner, &["status"], &[]);

    let here = std::fs::canonicalize(&inner).unwrap();
    let outer = std::fs::canonicalize(&root).unwrap();
    assert!(
        ran.out.lines().nth(1).unwrap()
            == format!(
                "you are standing in {}, which is a plane too — charter is acting on {}",
                here.display(),
                outer.display()
            ),
        "{}",
        ran.out
    );
    // And the cwd rung decided, because the tree being stood in is inside `alpha`.
    assert!(ran.out.contains("active: alpha (via cwd)"), "{}", ran.out);
}

#[test]
fn a_repo_the_manifest_names_with_nothing_on_disk_is_not_drawn_as_a_clone() {
    // Membership is not presence. The panel draws a member nobody has cloned; `status` draws
    // the directory, so a hand-made clone IS a row and a member with nothing on disk is not.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &["alpha"]);
    std::fs::write(
        root.join("workspaces/alpha/workspace.json"),
        r#"{"name": "alpha", "repos": [{"name": "absent", "branch": "main"}]}"#,
    )
    .unwrap();
    // A directory that is not a repository at all is not a row either.
    std::fs::create_dir_all(root.join("workspaces/alpha/plaindir")).unwrap();

    // `-w`, because standing at the plane root with no pointers the ladder ends on
    // `default` — which is not the workspace this test planted anything in.
    let ran = run(&root, &["status", "-w", "alpha"]);

    assert_eq!(ran.err, "", "nothing was refused");
    assert!(
        ran.out
            .contains("— workspace: alpha (active) · 0 repo(s) —"),
        "{}",
        ran.out
    );
    assert!(!ran.out.contains("absent"), "{}", ran.out);
    assert!(!ran.out.contains("plaindir"), "{}", ran.out);
}

#[test]
fn docs_generate_writes_the_topology_and_leaves_an_unmarked_readme_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &[]);
    inventory(
        &root,
        r#"{"group": "acme", "count": 1, "repos": [{"name": "widget", "kind": "app",
           "stack": "rust", "default_branch": "main", "description": "", "forge": "github"}]}"#,
    );
    std::fs::write(root.join("README.md"), "# Mine\n").unwrap();

    let ran = run(&root, &["docs", "generate"]);

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.err.contains("✓ Generated docs/topology.md (1 repos)"),
        "{}",
        ran.err
    );
    assert_eq!(
        std::fs::read_to_string(root.join("README.md")).unwrap(),
        "# Mine\n"
    );
    assert!(
        std::fs::read_to_string(root.join("docs/topology.md"))
            .unwrap()
            .contains("| `widget` | app | rust | `main` |  |")
    );
}

#[test]
fn bare_docs_is_the_same_command_as_docs_generate() {
    // Makefiles in the wild call it that way; making the group require a subcommand would
    // refuse a command line planes already have.
    let tmp = tempfile::tempdir().unwrap();
    let root = plane(tmp.path().join("plane"), &[]);
    let ran = run(&root, &["docs"]);

    assert_eq!(ran.code, 1);
    assert!(
        ran.err
            .contains("! Inventory is empty — run `charter discover` first."),
        "{}",
        ran.err
    );
}
