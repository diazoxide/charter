//! FM-9 (#1112): **a reference handed to a chat names only a path the core confined.**
//!
//! The window names a branch and a path inside it; [`reference::of`] places it with
//! `files::place` — inside the branch's folder, no `..`, through no link, never `.git` — and
//! builds the reference from the resolved path, relative to the chat's own folder when the
//! file is inside it. The text a harness is handed is rendered from that, never from a string
//! the window sent, and a name that would not type as one line is refused.

mod support;

use purlis_core::files::Branch;
use purlis_core::harness::Harness;
use purlis_core::reference::{self, Lines, NotReferable, Reference};
use purlis_core::worktree;

fn cut(f: &support::Fixture) -> std::path::PathBuf {
    let path = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect("a piece is cut")
        .path;
    std::fs::canonicalize(path).unwrap()
}

fn of(
    f: &support::Fixture,
    path: &str,
    lines: Option<Lines>,
    from: Option<&std::path::Path>,
) -> Result<Reference, NotReferable> {
    reference::of(
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "piece"),
        path,
        lines,
        from,
    )
}

fn refused(f: &support::Fixture, path: &str) -> String {
    match of(f, path, None, None) {
        Ok(made) => panic!("{path:?} was referenced as {made:?}"),
        Err(why) => why.to_string(),
    }
}

#[test]
fn a_file_is_named_relative_to_a_chat_working_in_its_branch() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    std::fs::create_dir_all(piece.join("src")).unwrap();
    std::fs::write(piece.join("src/main.rs"), "fn main() {}\n").unwrap();

    let made = of(
        &f,
        "src/main.rs",
        Some(Lines { first: 1, last: 1 }),
        Some(&piece),
    )
    .unwrap();

    assert_eq!(Harness::ClaudeCode.reference(&made), "@src/main.rs#L1");
    assert_eq!(Harness::Codex.reference(&made), "src/main.rs:1");
    assert_eq!(Harness::Opencode.reference(&made), "@src/main.rs#1");
}

#[test]
fn a_folder_is_named_as_a_folder() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    std::fs::create_dir_all(piece.join("src")).unwrap();

    let made = of(&f, "src", None, Some(&piece)).unwrap();

    assert!(made.folder());
    assert_eq!(Harness::ClaudeCode.reference(&made), "@src/");
}

#[test]
fn a_chat_working_elsewhere_is_handed_the_whole_path() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    let elsewhere = tempfile::tempdir().unwrap();

    let made = of(&f, "README.md", None, Some(elsewhere.path())).unwrap();
    let unknown = of(&f, "README.md", None, None).unwrap();

    let whole = piece.join("README.md").display().to_string();
    assert_eq!(made.path(), whole);
    assert_eq!(unknown.path(), whole);
}

#[test]
fn a_path_out_of_the_branch_through_git_or_a_link_is_refused() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc", piece.join("out")).unwrap();

    for path in [
        "../thing/README.md",
        "/etc/passwd",
        ".git/config",
        "",
        "nothing-here",
    ] {
        let _ = refused(&f, path);
    }
    #[cfg(unix)]
    {
        let _ = refused(&f, "out");
        let _ = refused(&f, "out/passwd");
    }
}

#[test]
fn a_name_with_a_line_break_is_refused_rather_than_typed() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    std::fs::write(piece.join("two\nlines"), "x").unwrap();

    assert!(refused(&f, "two\nlines").contains("control or invisible character"));
}

#[test]
fn a_name_drawn_as_something_else_is_refused() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    for name in ["x\u{202E}gnp.rs", "zero\u{200B}width", "line\u{2028}sep"] {
        std::fs::write(piece.join(name), "x").unwrap();

        assert!(
            refused(&f, name).contains("invisible character"),
            "{name:?} was not refused"
        );
    }
}

#[test]
fn a_name_a_harness_would_read_as_a_shell_command_is_written_from_here() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    std::fs::write(piece.join("!touch${IFS}pwned"), "x").unwrap();

    let made = of(&f, "!touch${IFS}pwned", None, Some(&piece)).unwrap();

    assert_eq!(made.path(), "./!touch${IFS}pwned");
    for harness in Harness::ALL {
        let rendered = harness.reference(&made);
        assert!(
            reference::starts_safely(&rendered),
            "{harness:?} renders {rendered}"
        );
    }
}

#[test]
fn lines_name_part_of_a_file_counted_from_one() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    std::fs::create_dir_all(piece.join("src")).unwrap();

    let folder = of(&f, "src", Some(Lines { first: 1, last: 2 }), None);
    let zero = of(&f, "README.md", Some(Lines { first: 0, last: 2 }), None);
    let backwards = of(&f, "README.md", Some(Lines { first: 5, last: 2 }), None);

    assert!(matches!(folder, Err(NotReferable::LinesOfAFolder(_))));
    assert!(matches!(zero, Err(NotReferable::NotARange { .. })));
    assert!(matches!(backwards, Err(NotReferable::NotARange { .. })));
}
