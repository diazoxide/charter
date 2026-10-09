//! FM-4 (#1107): **a branch marks what it changed**, file by file and rolled up onto its
//! folders, against the branch it was cut from (#1103, V86 F6). The same confinement as the
//! tree (`a_branch_expands_into_its_files.rs`): the window names a branch, never a directory,
//! and every path answered is inside the branch.

mod support;

use purlis_core::files::{self, Branch, Change, Mark};
use purlis_core::worktree;

/// The bounded reader, as the app starts it — but this test binary run again, picking
/// [`reader_child`], rather than the app's.
fn reader() -> files::Reader {
    files::Reader::new(
        std::env::current_exe().expect("the test binary"),
        [
            "reader_child",
            "--exact",
            "--nocapture",
            "--test-threads=1",
            files::READ_ARG,
        ]
        .map(std::ffi::OsString::from),
    )
}

/// The reader's child: in a run of this binary that [`reader`] started, it answers the one
/// question it was asked and exits; in any other run it does nothing.
#[test]
fn reader_child() {
    purlis_core::unsteered!();
    if let Some(code) = files::serve_if_asked() {
        std::process::exit(code);
    }
}

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
    purlis_core::unsteered!();
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

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

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
    // A folder holding more than one kind of change is changed. `src/` holds three: the file
    // committed in it, the one deleted from it, and the one renamed out of it, which it lost
    // (#1130, D-1130-1). The rename is not above both ends there, so it is not the rename's.
    assert_eq!(rolled(&status, "src"), Some((Mark::Changed, 3)));
    // The branch's own folder rolls up everything, the rename once: it is above both ends.
    assert_eq!(rolled(&status, ""), Some((Mark::Changed, 5)));
    assert_eq!(rolled(&status, "nowhere"), None);
    assert_eq!(status.base.as_deref(), Some("main"));
}

#[test]
fn what_is_still_uncommitted_is_told_apart_from_what_the_branch_committed() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "committed.txt", "in a commit\n");
    support::git(&piece, &["add", "-A"]);
    support::git(&piece, &["commit", "-q", "-m", "committed"]);
    write(&piece, "loose.txt", "not yet\n");

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

    let uncommitted: Vec<(&str, bool)> = status
        .changes
        .iter()
        .map(|one| (one.path.as_str(), one.uncommitted))
        .collect();
    assert_eq!(uncommitted, [("committed.txt", false), ("loose.txt", true)]);
}

#[test]
fn changed_only_is_exactly_what_the_branch_changed_against_its_base_committed_or_not() {
    purlis_core::unsteered!();
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

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

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
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

    assert!(status.changes.is_empty(), "{:?}", status.changes);
    assert!(status.folders.is_empty(), "{:?}", status.folders);
    assert_eq!(status.more, 0);
}

#[test]
fn the_repos_own_folder_has_no_recorded_base_so_it_marks_what_is_not_committed() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    f.commit(&f.clone, "committed.txt");
    write(&f.clone, "README.md", "changed\n");

    let status = files::status(&reader(), &f.plane, Branch::repo(&f.ws, &f.repo)).unwrap();

    assert_eq!(marks(&status.changes), [("README.md", Mark::Changed, None)]);
    assert_eq!(status.base, None);
}

/// The repo's own folder on `main`, following `<remote>/main` (#1130): the upstream holds one
/// commit `main` lacks (`theirs`), and `main` one the upstream lacks (`local`). No network: the
/// remote-tracking ref is written where a fetch would have left it.
fn following_an_upstream(f: &support::Fixture, remote: &str) {
    f.commit(&f.clone, "pushed");
    support::git(&f.clone, &["checkout", "-q", "-b", "elsewhere"]);
    f.commit(&f.clone, "theirs");
    support::git(
        &f.clone,
        &["update-ref", &format!("refs/remotes/{remote}/main"), "HEAD"],
    );
    support::git(&f.clone, &["checkout", "-q", "main"]);
    support::git(&f.clone, &["branch", "-q", "-D", "elsewhere"]);
    f.commit(&f.clone, "local");
    support::git(&f.clone, &["config", "branch.main.remote", remote]);
    support::git(
        &f.clone,
        &["config", "branch.main.merge", "refs/heads/main"],
    );
}

#[test]
fn the_repos_own_folder_is_marked_against_its_upstream() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    following_an_upstream(&f, "origin");
    write(&f.clone, "README.md", "changed\n");

    let status = files::status(&reader(), &f.plane, Branch::repo(&f.ws, &f.repo)).unwrap();

    // What `main` has that the upstream lacks, committed or not; never what the upstream
    // gained since (`theirs`).
    assert_eq!(
        marks(&status.changes),
        [
            ("README.md", Mark::Changed, None),
            ("local", Mark::Added, None)
        ]
    );
    assert_eq!(status.base.as_deref(), Some("origin/main"));
}

#[test]
fn an_upstream_whose_remote_is_not_one_plain_name_is_no_base() {
    purlis_core::unsteered!();
    for remote in ["a/b", "a..b"] {
        let f = support::plane_with_clone("thing");
        // `a..b` cannot be a ref git writes, so only the config names it.
        if remote.contains("..") {
            f.commit(&f.clone, "local");
            support::git(&f.clone, &["config", "branch.main.remote", remote]);
            support::git(
                &f.clone,
                &["config", "branch.main.merge", "refs/heads/main"],
            );
        } else {
            following_an_upstream(&f, remote);
        }
        write(&f.clone, "README.md", "changed\n");

        let status = files::status(&reader(), &f.plane, Branch::repo(&f.ws, &f.repo)).unwrap();

        assert_eq!(
            marks(&status.changes),
            [("README.md", Mark::Changed, None)],
            "{remote}"
        );
        assert_eq!(status.base, None, "{remote}");
    }
}

#[test]
fn an_upstream_that_names_no_ref_is_no_base() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    support::git(&f.clone, &["config", "branch.main.remote", "origin"]);
    support::git(
        &f.clone,
        &["config", "branch.main.merge", "refs/heads/main"],
    );
    write(&f.clone, "README.md", "changed\n");

    let status = files::status(&reader(), &f.plane, Branch::repo(&f.ws, &f.repo)).unwrap();

    assert_eq!(marks(&status.changes), [("README.md", Mark::Changed, None)]);
    assert_eq!(status.base, None);
}

#[test]
fn an_ignored_file_is_never_marked() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, ".gitignore", ".env\n");
    write(&piece, ".env", "API_TOKEN=sk-live-0123456789abcdef\n");

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

    assert_eq!(marks(&status.changes), [(".gitignore", Mark::Added, None)]);
}

#[test]
fn a_repository_nested_in_the_branch_is_not_marked_and_draws_no_empty_name() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let nested = piece.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    support::git(&nested, &["init", "-q", "-b", "main", "."]);
    write(&nested, "inside.txt", "another repository's\n");
    write(&piece, "loose.txt", "not committed\n");

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

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
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let said = files::status(
        &reader(),
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "nowhere"),
    )
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
    purlis_core::unsteered!();
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

    files::status(&reader(), &f.plane, branch(&f)).unwrap();

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
    purlis_core::unsteered!();
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

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

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

/// The clone's config, written whole and renamed into place, as an agent swapping it would.
#[cfg(unix)]
fn swap_config(f: &support::Fixture, text: &str, name: &str) {
    let staged = f.clone.join(".git").join(name);
    std::fs::write(&staged, text).unwrap();
    std::fs::rename(&staged, f.clone.join(".git/config")).unwrap();
}

#[cfg(unix)]
#[test]
fn many_reads_while_the_config_is_swapped_never_start_a_program_it_names() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_whose_files_name_filters(&f);
    let (program, ran) = filter(&f, "any");
    let clean = std::fs::read_to_string(f.clone.join(".git/config")).unwrap();
    // Every kind of program a repository's config can name for a read to run: a filter
    // driver's clean and its long-running process, an fsmonitor, and a promisor remote whose
    // upload-pack a lazy fetch would start.
    let named = format!(
        "{clean}[filter \"x\"]\n\tclean = {p}\n\tprocess = {p}\n\trequired = true\n\
         [filter \"y\"]\n\tclean = {p}\n[core]\n\tfsmonitor = {p}\n\
         [remote \"origin\"]\n\turl = {clone}\n\tpromisor = true\n\tuploadpack = {p}\n\
         [extensions]\n\tpartialClone = origin\n",
        p = program.display(),
        clone = f.clone.display(),
    );
    swap_config(&f, &named, "config.named");
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let swapper = {
        let stop = std::sync::Arc::clone(&stop);
        let (clean, named) = (clean.clone(), named.clone());
        let f_clone = f.clone.clone();
        std::thread::spawn(move || {
            let mut turn = false;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let text = if turn { &named } else { &clean };
                let staged = f_clone.join(".git/config.swap");
                std::fs::write(&staged, text).unwrap();
                std::fs::rename(&staged, f_clone.join(".git/config")).unwrap();
                turn = !turn;
            }
        })
    };

    for _ in 0..40 {
        // Written again as it was, so each read has to look at the files' content.
        write(&piece, "a.txt", "a\n");
        write(&piece, "b.txt", "b\n");
        let _ = files::status(&reader(), &f.plane, branch(&f));
        let _ = files::root(&reader(), &f.plane, branch(&f)).map(|root| {
            root.matters(&reader(), &[root.path().join("a.txt")]);
            root.folders(&reader())
        });
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    swapper.join().unwrap();
    swap_config(&f, &named, "config.named");
    let settled = files::status(&reader(), &f.plane, branch(&f));

    assert!(!ran.exists(), "a program the repo's config names ran");
    assert!(settled.is_ok(), "{settled:?}");
}

#[test]
fn a_recorded_base_that_reads_as_an_option_is_never_handed_to_git_as_one() {
    purlis_core::unsteered!();
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

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

    assert!(!wrote.exists(), "git took the recorded base as an option");
    assert_eq!(status.base, None);
    assert_eq!(marks(&status.changes), [("loose.txt", Mark::Added, None)]);
}

#[test]
fn every_path_answered_is_a_plain_path_inside_the_branch() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    write(&piece, "a b/c\td.txt", "odd name\n");
    write(&piece, "naïve.txt", "composed\n");
    support::git(&piece, &["mv", "src/old.rs", "renamed.rs"]);

    let status = files::status(&reader(), &f.plane, branch(&f)).unwrap();

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
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    write(&f.clone, "src/lib.rs", "tracked\n");
    write(&f.clone, ".gitignore", "target/\n");
    support::git(&f.clone, &["add", "-A"]);
    support::git(&f.clone, &["commit", "-q", "-m", "src"]);
    let piece = cut(&f, "piece");
    write(&piece, "new/deeper/x.txt", "untracked\n");
    write(&piece, "target/debug/out", "a build's\n");

    let root = files::root(&reader(), &f.plane, branch(&f)).unwrap();
    let folders: Vec<String> = root
        .folders(&reader())
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
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    write(&f.clone, ".gitignore", "target/\n");
    support::git(&f.clone, &["add", "-A"]);
    support::git(&f.clone, &["commit", "-q", "-m", "ignore"]);
    let piece = cut(&f, "piece");
    let root = files::root(&reader(), &f.plane, branch(&f)).unwrap();
    let at = |path: &str| root.path().join(path);

    assert!(root.matters(&reader(), &[at("src/deep/first.rs")]));
    assert!(root.matters(&reader(), &[at("target/debug/out"), at("README.md")]));
    assert!(!root.matters(&reader(), &[at("target/debug/out")]));
    assert!(!root.matters(&reader(), &[at(".git")]));
    assert!(!root.matters(&reader(), &[f.plane.join("elsewhere.txt")]));
    let _ = piece;
}

#[test]
fn a_write_to_a_file_git_tracks_matters_even_under_an_ignore_pattern() {
    purlis_core::unsteered!();
    // A file added with `git add -f` is tracked whatever the ignore rules say: an edit to it is
    // a change `status` shows, so it is told (FD-11 review).
    let f = support::plane_with_clone("thing");
    write(&f.clone, ".gitignore", "*.log\n");
    write(&f.clone, "logs/kept.log", "one\n");
    support::git(&f.clone, &["add", ".gitignore"]);
    support::git(&f.clone, &["add", "-f", "logs/kept.log"]);
    support::git(&f.clone, &["commit", "-q", "-m", "a tracked log"]);
    let piece = cut(&f, "piece");
    let root = files::root(&reader(), &f.plane, branch(&f)).unwrap();
    let at = |path: &str| root.path().join(path);

    assert!(root.matters(&reader(), &[at("logs/kept.log")]));
    assert!(!root.matters(&reader(), &[at("logs/other.log")]));
    let _ = piece;
}

/// git, for the test's own setup, with `input` on its standard input; its answer, trimmed.
#[cfg(unix)]
fn git_with_input(dir: &std::path::Path, args: &[&str], input: &[u8]) -> String {
    use std::io::Write as _;
    let mut cmd = support::unsigned();
    cmd.arg("-C").arg(dir).args(args);
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = purlis_core::forklock::spawn(&mut cmd).unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A piece whose HEAD names a file whose content is in no object store, in a clone that says a
/// promisor remote holds what it lacks — and whose upload-pack is `program`. Staging a new file
/// beside the missing one makes a rename search look for the missing content.
#[cfg(unix)]
fn branch_missing_an_object(f: &support::Fixture, program: &std::path::Path) -> std::path::PathBuf {
    let piece = cut(f, "piece");
    let ghost = git_with_input(
        &piece,
        &["hash-object", "--stdin"],
        b"a file long enough to be found renamed, and never stored\n",
    );
    let listed = String::from_utf8(support::git(&piece, &["ls-tree", "HEAD"]).stdout).unwrap();
    let input = format!("{listed}100644 blob {ghost}\tghost.txt\n");
    let tree = git_with_input(&piece, &["mktree", "--missing"], input.as_bytes());
    let commit = String::from_utf8(
        support::git(&piece, &["commit-tree", &tree, "-p", "HEAD", "-m", "ghost"]).stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    support::git(&piece, &["update-ref", "HEAD", &commit]);
    // Close to the missing content and not equal to it: only reading both finds the rename.
    write(
        &piece,
        "renamed.txt",
        "a file long enough to be found renamed, and never stored!\n",
    );
    support::git(&piece, &["add", "renamed.txt"]);
    for (key, value) in [
        ("core.repositoryformatversion", "1".to_string()),
        ("extensions.partialClone", "origin".to_string()),
        ("remote.origin.url", f.clone.display().to_string()),
        ("remote.origin.promisor", "true".to_string()),
        ("remote.origin.uploadpack", program.display().to_string()),
    ] {
        support::git(&f.clone, &["config", key, &value]);
    }
    piece
}

#[cfg(unix)]
#[test]
fn a_read_that_lacks_an_object_fetches_nothing_and_runs_no_program() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let (program, ran) = program(&f, "uploadpack");
    branch_missing_an_object(&f, &program);

    for _ in 0..3 {
        let _ = files::status(&reader(), &f.plane, branch(&f));
    }

    assert!(
        !ran.exists(),
        "the read fetched through the repo's promisor remote"
    );
}

#[cfg(unix)]
#[test]
fn git_run_by_charter_never_fetches_an_object_it_lacks() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let (program, ran) = program(&f, "uploadpack");
    let piece = branch_missing_an_object(&f, &program);

    // What the explorer's tree and the light editor run in a branch's folder, and a status:
    // each through the hardened runner.
    let _ = files::list(&f.plane, branch(&f));
    let _ = files::tree(&f.plane, branch(&f), "");
    let _ = purlis_core::worktree::git::run(
        &piece,
        &["--no-optional-locks", "status", "--porcelain"],
        purlis_core::worktree::git::READ,
    );

    assert!(
        !ran.exists(),
        "git fetched through the repo's promisor remote"
    );
}

/// This process's resident memory, in bytes.
fn resident() -> u64 {
    memory_stats::memory_stats().map_or(0, |used| used.physical_mem as u64)
}

#[cfg(unix)]
#[test]
fn an_ignore_file_linked_to_an_endless_device_fails_the_read_and_never_swells_charter() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::os::unix::fs::symlink("/dev/zero", piece.join(".gitignore")).unwrap();
    std::fs::create_dir_all(piece.join("sub")).unwrap();
    std::os::unix::fs::symlink("/dev/zero", piece.join("sub/.gitignore")).unwrap();
    let reader = reader().memory(256 * 1024 * 1024);
    let before = resident();

    let status = files::status(&reader, &f.plane, branch(&f));
    let root = files::root(&reader, &f.plane, branch(&f)).unwrap();
    let matters = root.matters(&reader, &[root.path().join("sub/new.txt")]);

    assert!(status.is_err(), "{status:?}");
    // A read that could not answer counts as one that matters: the markers are read again.
    assert!(matters);
    let grew = resident().saturating_sub(before);
    assert!(grew < 64 * 1024 * 1024, "charter grew by {grew} bytes");
}

#[cfg(unix)]
#[test]
fn a_fifo_named_as_an_ignore_file_times_the_read_out_and_other_branches_still_read() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    cut(&f, "other");
    let made = purlis_core::forklock::output(
        std::process::Command::new("mkfifo").arg(piece.join(".gitignore")),
    )
    .unwrap();
    assert!(made.status.success());
    let reader = reader().deadline(std::time::Duration::from_secs(3));

    let started = std::time::Instant::now();
    let hung = files::status(&reader, &f.plane, branch(&f));
    let took = started.elapsed();
    let other = files::status(&reader, &f.plane, Branch::piece(&f.ws, &f.repo, "other"));

    assert!(hung.is_err(), "{hung:?}");
    assert!(took < std::time::Duration::from_secs(10), "took {took:?}");
    assert!(other.is_ok(), "{other:?}");
}

#[test]
fn many_reads_of_a_branch_with_many_untracked_files_leave_charters_memory_steady() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    for folder in 0..100 {
        let at = piece.join(format!("gen/{folder}"));
        std::fs::create_dir_all(&at).unwrap();
        for file in 0..200 {
            std::fs::write(at.join(format!("{file}.txt")), "").unwrap();
        }
    }
    let reader = reader();
    // One read first, so what the first read sets up once is not counted.
    files::status(&reader, &f.plane, branch(&f)).unwrap();
    let before = resident();

    for _ in 0..15 {
        let status = files::status(&reader, &f.plane, branch(&f)).unwrap();
        assert_eq!(status.changes.len() + status.more, 20_000);
    }

    let grew = resident().saturating_sub(before);
    assert!(grew < 32 * 1024 * 1024, "charter grew by {grew} bytes");
}

/// What D-88f rests on is a property of the whole workspace, not of one `Cargo.toml`: a crate
/// anywhere that turned on gitoxide's network client, a transport or credential helpers would
/// unify those features into the reader. Read from cargo's own resolution of the workspace.
#[test]
fn gitoxide_has_no_network_client_transport_or_credential_helpers_anywhere_in_the_workspace() {
    purlis_core::unsteered!();
    // Only this host's platform: cargo has downloaded no package that only another platform
    // builds, and `--offline` would fail on one (main CI, 2026-10-04).
    let out = purlis_core::forklock::output(
        std::process::Command::new(env!("CARGO"))
            .args(["metadata", "--format-version", "1", "--locked", "--offline"])
            .args(["--filter-platform", host_triple()])
            .current_dir(env!("CARGO_MANIFEST_DIR")),
    )
    .expect("cargo metadata runs");
    // `--offline` reads only crates already downloaded, and a build of a few packages
    // (`cargo test -p purlis-core`) downloads only theirs (fedora job, main CI, 2026-10-04).
    assert!(
        out.status.success(),
        "run `cargo fetch --locked` first: this guard reads cargo's resolution of the whole \
         workspace offline, and a build of only some of its packages has not downloaded every \
         crate the lock names.\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let nodes = metadata["resolve"]["nodes"].as_array().unwrap();
    let crate_of = |id: &str| -> String {
        let named = id.rsplit('#').next().unwrap_or(id);
        named.split('@').next().unwrap_or(named).to_string()
    };
    let features = |name: &str| -> Option<Vec<String>> {
        nodes
            .iter()
            .find(|node| crate_of(node["id"].as_str().unwrap()) == name)
            .map(|node| {
                node["features"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|one| one.as_str().unwrap().to_string())
                    .collect()
            })
    };

    let gix = features("gix").expect("gix is in the workspace");
    let networked: Vec<&String> = gix
        .iter()
        .filter(|one| {
            one.contains("network")
                || one.contains("transport")
                || one.contains("credentials")
                || one.contains("worktree-mutation")
        })
        .collect();
    assert!(networked.is_empty(), "gix has {networked:?}");
    if let Some(transport) = features("gix-transport") {
        let clients: Vec<&String> = transport
            .iter()
            .filter(|one| one.contains("client"))
            .collect();
        assert!(clients.is_empty(), "gix-transport has {clients:?}");
    }
    for absent in ["gix-credentials", "gix-prompt"] {
        assert!(features(absent).is_none(), "{absent} is in the workspace");
    }
}

/// A child whose app quit or crashed while its read hung has nobody to kill it: it stops itself
/// at its own deadline, before the app's kill would have come.
#[cfg(unix)]
#[test]
fn a_hung_read_stops_itself_at_its_deadline_without_waiting_to_be_killed() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let made = purlis_core::forklock::output(
        std::process::Command::new("mkfifo").arg(piece.join(".gitignore")),
    )
    .unwrap();
    assert!(made.status.success());
    let deadline = std::time::Duration::from_secs(2);
    let reader = reader().deadline(deadline);

    let started = std::time::Instant::now();
    let said = files::status(&reader, &f.plane, branch(&f))
        .expect_err("a hung read answered")
        .to_string();
    let took = started.elapsed();

    // Stopped by itself — the app's kill says "did not finish" and comes a grace period later.
    assert!(said.contains("stopped without an answer"), "{said}");
    assert!(took < deadline + files::GRACE, "took {took:?}");
}

/// The target this test was built for, as cargo spells it, for `--filter-platform`.
fn host_triple() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        (arch, os) => panic!("no target triple known for {arch} on {os}"),
    }
}
