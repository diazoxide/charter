//! FM-4 (#1107): **a branch marks what it changed**, file by file and rolled up onto its
//! folders, against the branch it was cut from (#1103, V86 F6). The same confinement as the
//! tree (`a_branch_expands_into_its_files.rs`): the window names a branch, never a directory,
//! and every path answered is inside the branch.

mod support;

use charter_core::files::{self, Branch, Change, Mark};
use charter_core::worktree;

fn cut(f: &support::Fixture, piece: &str) -> std::path::PathBuf {
    worktree::add(&f.plane, &f.ws, &f.repo, piece, None)
        .expect("a piece is cut")
        .path
}

fn branch(f: &support::Fixture) -> Branch<'_> {
    Branch::piece(&f.ws, &f.repo, "piece")
}

fn write(at: &std::path::Path, path: &str, text: &str) {
    let file = at.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn marks(changes: &[Change]) -> Vec<(&str, Mark, Option<&str>)> {
    changes
        .iter()
        .map(|one| (one.path.as_str(), one.mark, one.from.as_deref()))
        .collect()
}

fn rolled(status: &files::Status, folder: &str) -> Option<(Mark, usize)> {
    status
        .folders
        .iter()
        .find(|one| one.folder == folder)
        .map(|one| (one.mark, one.count))
}

/// A clone whose `main` holds a few files to change, and a piece cut from it.
fn branch_with_files(f: &support::Fixture) -> std::path::PathBuf {
    write(&f.clone, "src/gone.rs", "gone\n");
    write(
        &f.clone,
        "src/old.rs",
        "a file long enough to be found renamed\n",
    );
    write(&f.clone, "lib/keep.rs", "keep\n");
    support::git(&f.clone, &["add", "-A"]);
    support::git(&f.clone, &["commit", "-q", "-m", "files"]);
    cut(f, "piece")
}

#[test]
fn a_changed_added_deleted_and_renamed_file_each_carry_their_mark_and_folders_roll_it_up() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    // Committed on the branch.
    write(&piece, "src/committed.rs", "new\n");
    support::git(&piece, &["add", "-A"]);
    support::git(&piece, &["commit", "-q", "-m", "committed"]);
    // Not committed.
    write(&piece, "README.md", "changed\n");
    write(&piece, "docs/new.md", "untracked\n");
    std::fs::remove_file(piece.join("src/gone.rs")).unwrap();
    support::git(&piece, &["mv", "src/old.rs", "lib/new.rs"]);

    let status = files::status(&f.plane, branch(&f)).unwrap();

    assert_eq!(
        marks(&status.changes),
        [
            ("README.md", Mark::Changed, None),
            ("docs/new.md", Mark::Added, None),
            ("lib/new.rs", Mark::Renamed, Some("src/old.rs")),
            ("src/committed.rs", Mark::Added, None),
            ("src/gone.rs", Mark::Deleted, None),
        ]
    );
    assert_eq!(rolled(&status, "docs"), Some((Mark::Added, 1)));
    assert_eq!(rolled(&status, "lib"), Some((Mark::Renamed, 1)));
    // A folder holding more than one kind of change is changed.
    assert_eq!(rolled(&status, "src"), Some((Mark::Changed, 2)));
    // The branch's own folder rolls up everything.
    assert_eq!(rolled(&status, ""), Some((Mark::Changed, 5)));
    assert_eq!(rolled(&status, "nowhere"), None);
    assert_eq!(status.base.as_deref(), Some("main"));
}

#[test]
fn what_is_still_uncommitted_is_told_apart_from_what_the_branch_committed() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "committed.txt", "in a commit\n");
    support::git(&piece, &["add", "-A"]);
    support::git(&piece, &["commit", "-q", "-m", "committed"]);
    write(&piece, "loose.txt", "not yet\n");

    let status = files::status(&f.plane, branch(&f)).unwrap();

    let uncommitted: Vec<(&str, bool)> = status
        .changes
        .iter()
        .map(|one| (one.path.as_str(), one.uncommitted))
        .collect();
    assert_eq!(uncommitted, [("committed.txt", false), ("loose.txt", true)]);
}

#[test]
fn changed_only_is_exactly_what_the_branch_changed_against_its_base_committed_or_not() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "mine.txt", "the branch's\n");
    support::git(&piece, &["add", "-A"]);
    support::git(&piece, &["commit", "-q", "-m", "mine"]);
    write(&piece, "loose.txt", "not committed\n");
    // Changed and changed back: nothing against the base.
    write(&piece, "README.md", "for a moment\n");
    write(&piece, "README.md", "one\n");
    // The base moves on after the cut: what it gained is not the branch's change.
    f.commit(&f.clone, "theirs.txt");

    let status = files::status(&f.plane, branch(&f)).unwrap();

    assert_eq!(
        marks(&status.changes),
        [
            ("loose.txt", Mark::Added, None),
            ("mine.txt", Mark::Added, None)
        ]
    );
}

#[test]
fn a_branch_with_nothing_changed_has_no_marks() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let status = files::status(&f.plane, branch(&f)).unwrap();

    assert!(status.changes.is_empty(), "{:?}", status.changes);
    assert!(status.folders.is_empty(), "{:?}", status.folders);
    assert_eq!(status.more, 0);
}

#[test]
fn the_repos_own_folder_has_no_recorded_base_so_it_marks_what_is_not_committed() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    f.commit(&f.clone, "committed.txt");
    write(&f.clone, "README.md", "changed\n");

    let status = files::status(&f.plane, Branch::repo(&f.ws, &f.repo)).unwrap();

    assert_eq!(marks(&status.changes), [("README.md", Mark::Changed, None)]);
    assert_eq!(status.base, None);
}

#[test]
fn an_ignored_file_is_never_marked() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, ".gitignore", ".env\n");
    write(&piece, ".env", "API_TOKEN=sk-live-0123456789abcdef\n");

    let status = files::status(&f.plane, branch(&f)).unwrap();

    assert_eq!(marks(&status.changes), [(".gitignore", Mark::Added, None)]);
}

#[test]
fn a_repository_nested_in_the_branch_is_not_marked_and_draws_no_empty_name() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let nested = piece.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    support::git(&nested, &["init", "-q", "-b", "main", "."]);
    write(&nested, "inside.txt", "another repository's\n");
    write(&piece, "loose.txt", "not committed\n");

    let status = files::status(&f.plane, branch(&f)).unwrap();

    assert_eq!(marks(&status.changes), [("loose.txt", Mark::Added, None)]);
    let folders: Vec<&str> = status
        .folders
        .iter()
        .map(|one| one.folder.as_str())
        .collect();
    assert_eq!(folders, [""]);
}

#[test]
fn a_status_refusal_never_says_worktree_piece_or_clone() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let said = files::status(&f.plane, Branch::piece(&f.ws, &f.repo, "nowhere"))
        .unwrap_err()
        .to_string()
        .to_lowercase();

    for word in ["worktree", "piece", "clone"] {
        assert!(!said.contains(word), "{said:?} says {word}");
    }
}

#[cfg(unix)]
fn program(f: &support::Fixture, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let marker = f.plane.join(format!("RAN-{name}"));
    let program = f.plane.join(format!("{name}.sh"));
    std::fs::write(&program, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (program, marker)
}

#[cfg(unix)]
#[test]
fn reading_a_branchs_marks_runs_no_hook_fsmonitor_or_external_diff_its_repos_config_names() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "README.md", "changed\n");
    let (monitor, monitored) = program(&f, "monitor");
    let (external, diffed) = program(&f, "external");
    let (hook, hooked) = program(&f, "hook");
    let hooks = f.plane.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for name in [
        "post-index-change",
        "reference-transaction",
        "post-checkout",
    ] {
        std::fs::copy(&hook, hooks.join(name)).unwrap();
    }
    support::git(
        &piece,
        &["config", "core.fsmonitor", &monitor.display().to_string()],
    );
    support::git(
        &piece,
        &["config", "diff.external", &external.display().to_string()],
    );
    support::git(
        &piece,
        &["config", "core.hooksPath", &hooks.display().to_string()],
    );

    files::status(&f.plane, branch(&f)).unwrap();

    assert!(
        !monitored.exists(),
        "the fsmonitor the repo's config names ran"
    );
    assert!(
        !diffed.exists(),
        "the external diff the repo's config names ran"
    );
    assert!(!hooked.exists(), "a hook the repo's config names ran");
}

/// A filter program: it records that it ran and passes the content through, as a real one
/// would, so git has an answer to compare.
#[cfg(unix)]
fn filter(f: &support::Fixture, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let marker = f.plane.join(format!("RAN-{name}"));
    let program = f.plane.join(format!("{name}.sh"));
    std::fs::write(
        &program,
        format!("#!/bin/sh\ntouch {}\ncat\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (program, marker)
}

/// A clone whose files each name a filter driver, with a piece cut from it whose files were
/// written again as they were: git has to run each file's clean filter to tell whether it
/// changed.
#[cfg(unix)]
fn branch_whose_files_name_filters(f: &support::Fixture) -> std::path::PathBuf {
    write(
        &f.clone,
        ".gitattributes",
        "a.txt filter=x\nb.txt filter=y\n",
    );
    write(&f.clone, "a.txt", "a\n");
    write(&f.clone, "b.txt", "b\n");
    support::git(&f.clone, &["add", "-A"]);
    support::git(&f.clone, &["commit", "-q", "-m", "filtered"]);
    let piece = cut(f, "piece");
    // A second later, so the files' times move and their sizes do not.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    write(&piece, "a.txt", "a\n");
    write(&piece, "b.txt", "b\n");
    write(&piece, "loose.txt", "not committed\n");
    piece
}

#[cfg(unix)]
#[test]
fn reading_a_branchs_marks_runs_no_filter_driver_its_repos_config_names() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_whose_files_name_filters(&f);
    let (direct, ran_direct) = filter(&f, "direct");
    let (included, ran_included) = filter(&f, "included");
    // One driver in the clone's own config, and one pulled in through an include: both are
    // the repo's, whoever can write the branch can write either.
    support::git(
        &f.clone,
        &["config", "filter.x.clean", &direct.display().to_string()],
    );
    support::git(
        &f.clone,
        &["config", "filter.x.process", &direct.display().to_string()],
    );
    let extra = f.plane.join("included.config");
    std::fs::write(
        &extra,
        format!("[filter \"y\"]\n\tclean = {}\n", included.display()),
    )
    .unwrap();
    support::git(
        &f.clone,
        &["config", "include.path", &extra.display().to_string()],
    );
    // And one named by the repo's own attributes file, which no tracked file shows.
    write(&f.clone, ".git/info/attributes", "loose.txt filter=x\n");

    let status = files::status(&f.plane, branch(&f)).unwrap();

    assert!(
        !ran_direct.exists(),
        "a filter driver in the repo's config ran"
    );
    assert!(
        !ran_included.exists(),
        "a filter driver the repo's config includes ran"
    );
    assert_eq!(
        marks(&status.changes),
        [("loose.txt", Mark::Added, None)],
        "a file rewritten as it was is not a change"
    );
    let _ = piece;
}

#[cfg(unix)]
#[test]
fn a_filter_driver_charter_cannot_turn_off_refuses_the_read_rather_than_running() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    branch_whose_files_name_filters(&f);
    let (odd, ran) = filter(&f, "odd");
    // A name with `=` in it cannot be spelled as a `-c` key, so it cannot be blanked.
    let config = f.clone.join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!("[filter \"x=y\"]\n\tclean = {}\n", odd.display()));
    std::fs::write(&config, text).unwrap();
    write(&f.clone, ".git/info/attributes", "a.txt filter=x=y\n");

    let refused = files::status(&f.plane, branch(&f));

    assert!(
        !ran.exists(),
        "a filter driver charter could not turn off ran"
    );
    let said = refused.expect_err("the read went ahead").to_string();
    assert!(said.contains("filter"), "{said}");
}

#[test]
fn a_recorded_base_that_reads_as_an_option_is_never_handed_to_git_as_one() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "loose.txt", "not committed\n");
    let wrote = f.plane.join("WROTE");
    // Anything in the branch can write the clone's config: the base is read, never trusted.
    let config = f.clone.join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text = text.replace(
        "charterBase = main",
        &format!("charterBase = --output={}", wrote.display()),
    );
    std::fs::write(&config, text).unwrap();

    let status = files::status(&f.plane, branch(&f)).unwrap();

    assert!(!wrote.exists(), "git took the recorded base as an option");
    assert_eq!(status.base, None);
    assert_eq!(marks(&status.changes), [("loose.txt", Mark::Added, None)]);
}

#[test]
fn every_path_answered_is_a_plain_path_inside_the_branch() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    write(&piece, "a b/c\td.txt", "odd name\n");
    write(&piece, "naïve.txt", "composed\n");
    support::git(&piece, &["mv", "src/old.rs", "renamed.rs"]);

    let status = files::status(&f.plane, branch(&f)).unwrap();

    let paths: Vec<&str> = status
        .changes
        .iter()
        .flat_map(|one| std::iter::once(one.path.as_str()).chain(one.from.as_deref()))
        .collect();
    assert!(paths.contains(&"a b/c\td.txt"), "{paths:?}");
    assert!(paths.contains(&"naïve.txt"), "{paths:?}");
    for path in paths {
        let relative = std::path::Path::new(path);
        assert!(relative.is_relative(), "{path}");
        assert!(
            relative
                .components()
                .all(|step| matches!(step, std::path::Component::Normal(_))),
            "{path}"
        );
        assert!(!path.split('/').any(|step| step == ".git"), "{path}");
    }
}

#[test]
fn the_folders_listened_in_are_the_ones_git_knows_and_never_what_it_ignores() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    write(&f.clone, "src/lib.rs", "tracked\n");
    write(&f.clone, ".gitignore", "target/\n");
    support::git(&f.clone, &["add", "-A"]);
    support::git(&f.clone, &["commit", "-q", "-m", "src"]);
    let piece = cut(&f, "piece");
    write(&piece, "new/deeper/x.txt", "untracked\n");
    write(&piece, "target/debug/out", "a build's\n");

    let root = files::root(&f.plane, branch(&f)).unwrap();
    let folders: Vec<String> = root
        .folders()
        .unwrap()
        .iter()
        .map(|one| {
            one.strip_prefix(root.path())
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    assert_eq!(folders, ["", "new", "src", "new/deeper"]);
}

#[test]
fn a_write_matters_unless_git_ignores_it_or_it_is_gits_own() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    write(&f.clone, ".gitignore", "target/\n");
    support::git(&f.clone, &["add", "-A"]);
    support::git(&f.clone, &["commit", "-q", "-m", "ignore"]);
    let piece = cut(&f, "piece");
    let root = files::root(&f.plane, branch(&f)).unwrap();
    let at = |path: &str| root.path().join(path);

    assert!(root.matters(&[at("src/deep/first.rs")]));
    assert!(root.matters(&[at("target/debug/out"), at("README.md")]));
    assert!(!root.matters(&[at("target/debug/out")]));
    assert!(!root.matters(&[at(".git")]));
    assert!(!root.matters(&[f.plane.join("elsewhere.txt")]));
    let _ = piece;
}
