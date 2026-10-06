//! FM-10 (#1113): **a file or folder row's path leaves the window only through the core.**
//!
//! Copy path, Reveal in your file manager and Open a shell tab here act on a path outside the
//! window. The window names a branch and a path inside it; [`files::place`] answers with the
//! resolved absolute path, or refuses. It follows no link, never leaves the branch, and never
//! names git's own `.git`. An ignored file or folder places like any other: the tree draws it
//! when the operator asks, and placing it hands the window no byte of it.

mod support;

use purlis_core::files::{self, Branch, Placed};
use purlis_core::worktree;

fn cut(f: &support::Fixture) -> std::path::PathBuf {
    let path = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect("a piece is cut")
        .path;
    std::fs::canonicalize(path).unwrap()
}

fn place(f: &support::Fixture, path: &str) -> Result<Placed, files::Refused> {
    files::place(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), path)
}

fn refused(f: &support::Fixture, path: &str) -> String {
    match place(f, path) {
        Ok(placed) => panic!("{path:?} was placed at {placed:?}"),
        Err(refused) => refused.to_string(),
    }
}

#[test]
fn a_file_of_a_branch_is_placed_at_its_resolved_absolute_path() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);

    let placed = place(&f, "README.md").unwrap();

    assert_eq!(
        placed,
        Placed {
            absolute: piece.join("README.md"),
            relative: "README.md".to_string(),
            folder: false,
        }
    );
}

#[test]
fn a_folder_of_a_branch_is_placed_and_said_to_be_a_folder() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    std::fs::create_dir_all(piece.join("src/deep")).unwrap();

    let placed = place(&f, "./src//deep").unwrap();

    assert_eq!(
        placed,
        Placed {
            absolute: piece.join("src/deep"),
            relative: "src/deep".to_string(),
            folder: true,
        }
    );
}

#[test]
fn an_ignored_folder_is_placed_like_any_other() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    std::fs::write(piece.join(".gitignore"), "target/\n").unwrap();
    std::fs::create_dir_all(piece.join("target")).unwrap();

    let placed = place(&f, "target").unwrap();

    assert_eq!(placed.absolute, piece.join("target"));
}

#[test]
fn the_repos_own_folder_places_its_files_too() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let clone = std::fs::canonicalize(&f.clone).unwrap();

    let placed = files::place(&f.plane, Branch::repo(&f.ws, &f.repo), "README.md").unwrap();

    assert_eq!(placed.absolute, clone.join("README.md"));
}

#[test]
fn a_path_outside_the_branch_is_refused() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f);

    assert_eq!(
        refused(&f, "../thing"),
        "'../thing' is not a path inside the branch's folder"
    );
    assert_eq!(
        refused(&f, "src/../../x"),
        "'src/../../x' is not a path inside the branch's folder"
    );
    assert_eq!(
        refused(&f, "/etc/hosts"),
        "'/etc/hosts' is not a path inside the branch's folder"
    );
    assert_eq!(
        refused(&f, ""),
        "'' is not a path inside the branch's folder"
    );
}

#[test]
fn gits_own_folder_is_refused() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f);

    // A worktree's `.git` is a file naming the clone's git directory; the clone's is a folder.
    assert_eq!(
        refused(&f, ".git"),
        "'.git' is not a path inside the branch's folder"
    );
    let clone = files::place(&f.plane, Branch::repo(&f.ws, &f.repo), ".git/config");
    assert_eq!(
        clone.unwrap_err().to_string(),
        "'.git/config' is not a path inside the branch's folder"
    );
}

#[cfg(unix)]
#[test]
fn no_link_is_followed_whether_it_leads_out_of_the_branch_or_stays_in_it() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "x").unwrap();
    std::os::unix::fs::symlink(outside.path(), piece.join("out")).unwrap();
    std::os::unix::fs::symlink(piece.join("README.md"), piece.join("CLAUDE.md")).unwrap();

    assert_eq!(
        refused(&f, "out"),
        "'out' is not a path inside the branch's folder"
    );
    assert_eq!(
        refused(&f, "out/secret.txt"),
        "'out/secret.txt' is not a path inside the branch's folder"
    );
    assert_eq!(
        refused(&f, "CLAUDE.md"),
        "'CLAUDE.md' is not a path inside the branch's folder"
    );
}

#[test]
fn a_path_that_is_not_there_is_said_so() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f);

    assert_eq!(
        refused(&f, "gone.txt"),
        "'gone.txt' is not in the branch's folder any more"
    );
}

#[test]
fn a_branch_the_workspace_does_not_have_is_refused() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");

    let refused =
        files::place(&f.plane, Branch::piece(&f.ws, &f.repo, "nope"), "README.md").unwrap_err();

    assert!(
        refused
            .to_string()
            .contains("no branch folder called 'nope'"),
        "{refused}"
    );
}

#[cfg(unix)]
#[test]
fn a_links_own_path_is_named_and_nothing_it_leads_to_is_placed() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f);
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), piece.join("out")).unwrap();
    std::os::unix::fs::symlink(piece.join("README.md"), piece.join("CLAUDE.md")).unwrap();
    let named = |path: &str| {
        files::named(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), path)
            .map_err(|refused| refused.to_string())
    };

    assert_eq!(named("CLAUDE.md"), Ok("CLAUDE.md".to_string()));
    assert_eq!(named("./out"), Ok("out".to_string()));
    assert!(place(&f, "CLAUDE.md").is_err());
    assert_eq!(
        named("out/anything"),
        Err("'out/anything' is not a path inside the branch's folder".to_string())
    );
    assert_eq!(
        named("gone.md"),
        Err("'gone.md' is not in the branch's folder any more".to_string())
    );
    assert_eq!(
        named("../x"),
        Err("'../x' is not a path inside the branch's folder".to_string())
    );
}

#[test]
fn gits_folder_is_refused_by_its_name_in_any_case_whether_or_not_anything_is_there() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let clone = Branch::repo(&f.ws, &f.repo);
    let said = |path: &str| files::place(&f.plane, clone, path).unwrap_err().to_string();

    // `hooks` is there and `nothing-here` is not: both get the same sentence.
    assert_eq!(
        said(".git/nothing-here"),
        "'.git/nothing-here' is not a path inside the branch's folder"
    );
    assert_eq!(
        said(".git/hooks"),
        "'.git/hooks' is not a path inside the branch's folder"
    );
    assert_eq!(
        said(".GIT/config"),
        "'.GIT/config' is not a path inside the branch's folder"
    );
    assert_eq!(
        files::folder(&f.plane, clone, ".Git/nothing-here")
            .unwrap_err()
            .to_string(),
        "'.Git/nothing-here' is not a path inside the branch's folder"
    );
    assert_eq!(
        files::named(&f.plane, clone, ".git/HEAD")
            .unwrap_err()
            .to_string(),
        "'.git/HEAD' is not a path inside the branch's folder"
    );
}
