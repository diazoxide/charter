//! `charter harness show <name>` — a harness's declaration as charter read it, built-in or
//! the project's (ADR 0073 §3, FD-14).
//!
//! It prints the file itself, under one comment line naming where it came from and the digest
//! an approval is of, so what it prints is a declaration a reader can copy into
//! `harnesses/<name>.toml` and start from.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn charter(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(args)
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        .output()
        .expect("the binary runs")
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), "").unwrap();
    dir
}

fn declare(root: &Path, name: &str, text: &str) {
    fs::create_dir_all(root.join("harnesses")).unwrap();
    fs::write(root.join("harnesses").join(format!("{name}.toml")), text).unwrap();
}

#[test]
fn a_built_in_is_shown_as_the_declaration_charter_ships() {
    let dir = project();

    let out = charter(dir.path(), &["harness", "show", "claude"]);

    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stdout);
    let first = said.lines().next().unwrap();
    assert!(first.starts_with("# claude: built-in, sha256:"), "{said}");
    assert!(said.contains("\nprogram = \"claude\"\n"), "{said}");
    assert!(said.contains("\nhooks = true\n"), "{said}");
}

#[test]
fn a_projects_declaration_is_shown_as_its_file_says_it() {
    let dir = project();
    declare(
        dir.path(),
        "aider",
        "name = \"aider\"\nprogram = \"aider\"\n\n[capabilities]\nreports_waiting = \"unknown\"\n",
    );

    let out = charter(dir.path(), &["harness", "show", "aider"]);

    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        said.starts_with("# aider: harnesses/aider.toml, sha256:"),
        "{said}"
    );
    assert!(
        said.ends_with(
            "\nname = \"aider\"\nprogram = \"aider\"\n\n[capabilities]\nreports_waiting = \"unknown\"\n"
        ),
        "{said}"
    );
}

#[test]
fn a_control_byte_in_a_declaration_never_reaches_the_terminal() {
    // TOML takes no raw control byte, so the file is refused, and the refusal is escaped.
    let dir = project();
    declare(
        dir.path(),
        "aider",
        "name = \"aider\"\nprogram = \"aider\"\ntested = \"1\u{1b}[2J\"\n",
    );

    let out = charter(dir.path(), &["harness", "show", "aider"]);

    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        !out.stdout.contains(&0x1b) && !out.stderr.contains(&0x1b),
        "{out:?}"
    );
}

#[test]
fn a_refused_declaration_is_not_shown_and_its_refusal_is() {
    let dir = project();
    declare(
        dir.path(),
        "aider",
        "name = \"aider\"\nprogram = \"/bin/aider\"\n",
    );

    let out = charter(dir.path(), &["harness", "show", "aider"]);

    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("harnesses/aider.toml") && said.contains("not a bare program name"),
        "{said}"
    );
}

#[test]
fn a_name_nothing_declares_says_which_harnesses_there_are() {
    let dir = project();
    declare(
        dir.path(),
        "aider",
        "name = \"aider\"\nprogram = \"aider\"\n",
    );

    let out = charter(dir.path(), &["harness", "show", "gemini"]);

    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "charter: no harness 'gemini' is declared — the harnesses are claude, opencode, codex, \
         aider. A project declares one in harnesses/<name>.toml.\n"
    );
}

#[test]
fn a_declared_harness_is_listed_with_the_profiles() {
    let dir = project();
    declare(
        dir.path(),
        "aider",
        "name = \"aider\"\nprogram = \"aider\"\n",
    );

    let out = charter(dir.path(), &["harness", "list"]);

    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("\n  aider     aider     aider     harnesses\n"),
        "{said}"
    );
}

#[test]
fn a_declaration_with_a_byte_outside_printable_ascii_says_it_is_shown_escaped() {
    let dir = project();
    declare(
        dir.path(),
        "aider",
        "# caf\u{e9}\nname = \"aider\"\nprogram = \"aider\"\n",
    );

    let out = charter(dir.path(), &["harness", "show", "aider"]);

    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.contains("\n# shown escaped: "), "{said}");
    assert!(!said.contains('\u{e9}'), "{said}");
    // A file that is all printable ASCII is printed as it is, with no such line.
    declare(
        dir.path(),
        "aider",
        "name = \"aider\"\nprogram = \"aider\"\n",
    );
    let plain = charter(dir.path(), &["harness", "show", "aider"]);
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("shown escaped"));
}
