//! `charter harness list` — every profile charter read, the file it came from, and why any
//! was refused.
//!
//! There is no `charter harness add`: a chat can run a command as easily as it can edit a
//! file, so a command could never stand for the operator's approval of what it wrote. This
//! listing is how an operator who edited `charter.local.toml` reads back what charter made
//! of it, so its shape is the Python charter's, taken from that command's own output.

use std::fs;
use std::path::Path;
use std::process::Command;

fn charter(root: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(["harness", "list"])
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        .output()
        .expect("the binary runs");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "",
        "the listing is a report, and charter prints its reports on stderr"
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn plane(local: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), "").unwrap();
    if !local.is_empty() {
        fs::write(dir.path().join("charter.local.toml"), local).unwrap();
    }
    dir
}

#[test]
fn a_plane_that_declares_nothing_lists_the_built_ins_in_registry_order() {
    // Byte for byte the Python charter's, columns sized from the cells rather than guessed.
    let dir = plane("");

    assert_eq!(
        charter(dir.path()),
        "  NAME      KIND      COMMAND   FROM\n\
         \x20 claude    claude    claude    built-in\n\
         \x20 opencode  opencode  opencode  built-in\n\
         \x20 codex     codex     codex     built-in\n"
    );
}

#[test]
fn a_declared_profile_is_listed_after_the_built_ins_with_the_file_it_came_from() {
    // And the default is marked with `*`, which is the row the selector starts on — it
    // launches nothing by itself.
    let dir = plane(
        "[harness]\ndefault = \"claude-work\"\n\n\
         [harness.claude-work]\nkind = \"claude\"\n\
         command = [\"claude\", \"--model\", \"opus\"]\n\
         env = { CLAUDE_CONFIG_DIR = \"~/.claude-work\" }\n\n\
         [harness.broken]\nkind = \"nope\"\ncommand = [\"x\"]\n",
    );

    assert_eq!(
        charter(dir.path()),
        "  NAME         KIND      COMMAND                                               FROM\n\
         \x20 claude       claude    claude                                                built-in\n\
         \x20 opencode     opencode  opencode                                              built-in\n\
         \x20 codex        codex     codex                                                 built-in\n\
         * claude-work  claude    CLAUDE_CONFIG_DIR=~/.claude-work claude --model opus  charter.local.toml\n\
         refused:\n\
         \x20 broken: profile 'broken' has kind nope, which is not a harness charter can launch — one of: claude, opencode, codex. Set kind to one of them.\n"
    );
}

#[test]
fn a_whole_file_refusal_is_listed_under_the_file_that_carried_it() {
    // A refusal with no profile name — the local file would not parse — is named by its
    // FILE, so the line is never `: <reason>` with nothing in front of the colon.
    let dir = plane("this is not toml\n");

    let out = charter(dir.path());

    assert!(
        out.contains("\nrefused:\n  charter.local.toml: charter.local.toml could not be read ("),
        "{out}"
    );
}

#[test]
fn a_plane_whose_profiles_git_would_carry_lists_them_refused_and_names_the_one_fix() {
    // Not ordinary rows with a warning under them: each of those refusals says "charter
    // reads nothing in it", so the listing has to show them that way.
    let dir = plane("[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n");
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .output()
        .expect("git runs");

    let out = charter(dir.path());

    assert!(
        !out.contains("\n  work "),
        "the profile was listed as usable:\n{out}"
    );
    assert!(
        out.contains("\n  work: git would commit charter.local.toml,"),
        "{out}"
    );
    assert!(
        out.ends_with("! to use the profiles in charter.local.toml: charter reinit\n"),
        "{out}"
    );
}
