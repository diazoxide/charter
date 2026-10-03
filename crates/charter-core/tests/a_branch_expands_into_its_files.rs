//! FM-1 (#1104): **a branch expands into its files**, one folder at a time, in the explorer's
//! tree (#1103, V86). The same confinement as the light editor
//! (`any_file_in_a_piece_opens_in_the_light_editor.rs`): the window names a branch and a folder
//! relative to it, never a directory, and what the tree says opens is what opens.

mod support;

use charter_core::files::{self, Branch, Entry, Kind};
use charter_core::worktree;

fn cut(f: &support::Fixture, piece: &str) -> std::path::PathBuf {
    worktree::add(&f.plane, &f.ws, &f.repo, piece, None)
        .expect("a piece is cut")
        .path
}

fn branch(f: &support::Fixture) -> Branch<'_> {
    Branch::piece(&f.ws, &f.repo, "piece")
}

fn names(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|one| one.name.as_str()).collect()
}

fn entry<'a>(entries: &'a [Entry], name: &str) -> &'a Entry {
    entries
        .iter()
        .find(|one| one.name == name)
        .unwrap_or_else(|| panic!("no {name} in {entries:?}"))
}

#[test]
fn expanding_a_branch_lists_its_first_level_with_folders_first_in_natural_order() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::create_dir_all(piece.join("src/deep")).unwrap();
    std::fs::write(piece.join("src/deep/lib.rs"), "\n").unwrap();
    std::fs::create_dir_all(piece.join("Docs")).unwrap();
    std::fs::write(piece.join("Docs/guide.md"), "\n").unwrap();
    std::fs::write(piece.join("note10.md"), "\n").unwrap();
    std::fs::write(piece.join("note2.md"), "\n").unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;

    // `.git` is the worktree's own, and hidden with what git ignores.
    let shown: Vec<Entry> = top.iter().filter(|one| !one.ignored).cloned().collect();

    assert_eq!(
        names(&shown),
        ["Docs", "src", "note2.md", "note10.md", "README.md"],
        "{top:?}"
    );
    assert_eq!(entry(&top, "src").kind, Kind::Folder);
    assert_eq!(entry(&top, "README.md").kind, Kind::File);
    assert!(shown.iter().all(|one| one.refused.is_none()), "{top:?}");
}

#[test]
fn expanding_a_folder_lists_the_next_level_and_nothing_deeper() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::create_dir_all(piece.join("src/deep")).unwrap();
    std::fs::write(piece.join("src/deep/lib.rs"), "\n").unwrap();
    std::fs::write(piece.join("src/main.rs"), "\n").unwrap();

    let src = files::tree(&f.plane, branch(&f), "src").unwrap().entries;
    let deep = files::tree(&f.plane, branch(&f), "src/deep")
        .unwrap()
        .entries;

    assert_eq!(names(&src), ["deep", "main.rs"]);
    assert_eq!(names(&deep), ["lib.rs"]);
}

#[test]
fn an_ignored_file_is_named_marked_ignored_and_refused_with_its_reason() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join(".gitignore"), "target/\n.env\n").unwrap();
    std::fs::create_dir_all(piece.join("target")).unwrap();
    std::fs::write(piece.join("target/out.bin"), "built\n").unwrap();
    std::fs::write(piece.join(".env"), "API_TOKEN=sk-live-0123456789abcdef\n").unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;
    let inside = files::tree(&f.plane, branch(&f), "target").unwrap().entries;

    let secret = entry(&top, ".env");
    assert!(secret.ignored, "{secret:?}");
    let why = secret
        .refused
        .as_deref()
        .expect("an ignored file is refused");
    assert!(why.contains("ignore"), "{why}");
    assert!(!why.contains("sk-live"), "{why}");
    let target = entry(&top, "target");
    assert!(
        target.ignored && target.refused.is_none(),
        "an ignored folder still expands: {target:?}"
    );
    assert!(!entry(&top, ".gitignore").ignored);
    assert!(!entry(&top, "README.md").ignored);
    let built = entry(&inside, "out.bin");
    assert!(
        built.ignored && built.refused.is_some(),
        "a file in an ignored folder is ignored: {built:?}"
    );
}

#[test]
fn a_tracked_file_under_an_ignored_pattern_is_not_ignored_because_git_tracks_it() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join("kept.log"), "tracked\n").unwrap();
    support::git(&piece, &["add", "kept.log"]);
    std::fs::write(piece.join(".gitignore"), "*.log\n").unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;

    let kept = entry(&top, "kept.log");
    assert!(!kept.ignored && kept.refused.is_none(), "{kept:?}");
}

#[test]
fn gits_own_folder_is_drawn_refused_and_does_not_expand() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    // The repo's own folder, where `.git` is a folder rather than a worktree's file.
    let top = files::tree(&f.plane, Branch::repo(&f.ws, &f.repo), "")
        .unwrap()
        .entries;

    let git = entry(&top, ".git");
    assert!(git.ignored, "hidden with what git ignores: {git:?}");
    assert!(
        git.refused
            .as_deref()
            .is_some_and(|why| why.contains("git")),
        "{git:?}"
    );
    for folder in [".git", ".git/refs", "./.git"] {
        let refused = files::tree(&f.plane, Branch::repo(&f.ws, &f.repo), folder);
        assert!(refused.is_err(), "{folder:?} expanded: {refused:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_branch_a_fifo_and_a_link_to_an_ignored_file_are_refused_with_reasons() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let outside = f.plane.join("outside.txt");
    std::fs::write(&outside, "not the branch's\n").unwrap();
    std::os::unix::fs::symlink(&outside, piece.join("away.txt")).unwrap();
    std::os::unix::fs::symlink("README.md", piece.join("AGENTS.md")).unwrap();
    std::fs::write(piece.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(piece.join(".env"), "API_TOKEN=x\n").unwrap();
    std::os::unix::fs::symlink(".env", piece.join("settings.txt")).unwrap();
    let made = charter_core::forklock::output(
        std::process::Command::new("mkfifo").arg(piece.join("pipe")),
    )
    .unwrap();
    assert!(made.status.success(), "{made:?}");

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;

    let away = entry(&top, "away.txt");
    assert_eq!(away.kind, Kind::Link);
    assert!(
        away.refused
            .as_deref()
            .is_some_and(|why| why.contains("out of")),
        "{away:?}"
    );
    let agents = entry(&top, "AGENTS.md");
    assert_eq!(agents.kind, Kind::Link);
    assert_eq!(
        agents.refused, None,
        "a link to a file of the same branch opens"
    );
    let settings = entry(&top, "settings.txt");
    assert!(
        settings
            .refused
            .as_deref()
            .is_some_and(|why| why.contains("ignore")),
        "{settings:?}"
    );
    let pipe = entry(&top, "pipe");
    assert!(
        pipe.refused
            .as_deref()
            .is_some_and(|why| why.contains("not a file")),
        "{pipe:?}"
    );
}

#[test]
fn a_folder_that_leaves_the_branch_is_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    for folder in ["..", "../..", "/etc", "src/../..", "nowhere", "README.md"] {
        let refused = files::tree(&f.plane, branch(&f), folder);
        assert!(refused.is_err(), "{folder:?} expanded: {refused:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_linked_folder_is_not_expanded_even_when_it_stays_inside() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::create_dir_all(f.plane.join("elsewhere")).unwrap();
    std::fs::write(f.plane.join("elsewhere/secret.txt"), "x\n").unwrap();
    std::os::unix::fs::symlink(f.plane.join("elsewhere"), piece.join("docs")).unwrap();
    std::fs::create_dir_all(piece.join("real")).unwrap();
    std::os::unix::fs::symlink("real", piece.join("alias")).unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;

    assert_eq!(entry(&top, "docs").kind, Kind::Link);
    assert!(files::tree(&f.plane, branch(&f), "docs").is_err());
    assert!(files::tree(&f.plane, branch(&f), "alias").is_err());
}

#[test]
fn nothing_inside_a_repository_nested_in_the_branch_opens() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let nested = piece.join("sub");
    std::fs::create_dir_all(&nested).unwrap();
    support::git(&nested, &["init", "-q", "."]);
    std::fs::write(nested.join("note.txt"), "hello\n").unwrap();

    let sub = files::tree(&f.plane, branch(&f), "sub").unwrap().entries;

    let note = entry(&sub, "note.txt");
    assert!(note.refused.is_some(), "{note:?}");
    assert!(
        files::open(&f.plane, branch(&f), "sub/note.txt").is_err(),
        "the light editor agrees"
    );
}

#[cfg(unix)]
#[test]
fn what_the_tree_says_opens_opens_and_what_it_refuses_does_not() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::create_dir_all(piece.join("src")).unwrap();
    std::fs::write(piece.join("src/lib.rs"), "\n").unwrap();
    std::fs::write(piece.join(".gitignore"), ".env\nbuild/\n").unwrap();
    std::fs::write(piece.join(".env"), "x\n").unwrap();
    std::fs::create_dir_all(piece.join("build")).unwrap();
    std::fs::write(piece.join("build/out"), "x\n").unwrap();
    std::os::unix::fs::symlink("README.md", piece.join("AGENTS.md")).unwrap();
    std::os::unix::fs::symlink(".env", piece.join("env.txt")).unwrap();
    std::os::unix::fs::symlink(f.plane.join("charter.toml"), piece.join("away")).unwrap();

    for folder in ["", "src", "build"] {
        for one in files::tree(&f.plane, branch(&f), folder).unwrap().entries {
            if one.kind == Kind::Folder {
                continue;
            }
            let path = if folder.is_empty() {
                one.name.clone()
            } else {
                format!("{folder}/{}", one.name)
            };
            let opened = files::open(&f.plane, branch(&f), &path);
            assert_eq!(
                opened.is_ok(),
                one.refused.is_none(),
                "{path}: the tree said {:?}, the open answered {opened:?}",
                one.refused
            );
        }
    }
}

#[test]
fn a_repo_row_expands_into_the_repos_own_files_with_the_same_containment() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    std::fs::write(f.clone.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(f.clone.join(".env"), "API_TOKEN=x\n").unwrap();
    std::fs::write(f.plane.join("charter.local.toml"), "secret = 1\n").unwrap();
    let repo = Branch::repo(&f.ws, &f.repo);

    let top = files::tree(&f.plane, repo, "").unwrap().entries;
    let readme = files::open(&f.plane, repo, "README.md").unwrap();

    assert!(names(&top).contains(&"README.md"), "{top:?}");
    assert!(entry(&top, ".env").ignored);
    assert_eq!(
        readme,
        files::Opened::Text {
            text: "one\n".to_string()
        }
    );
    assert!(files::open(&f.plane, repo, ".env").is_err());
    assert!(files::open(&f.plane, repo, "../../charter.local.toml").is_err());
    assert!(files::tree(&f.plane, repo, "..").is_err());
}

#[test]
fn a_repo_name_that_walks_out_of_the_workspace_is_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");

    for repo in ["..", "../alpha", "thing/..", ""] {
        let refused = files::tree(&f.plane, Branch::repo(&f.ws, repo), "");
        assert!(refused.is_err(), "{repo:?} expanded: {refused:?}");
    }
}

#[test]
fn a_folder_inside_the_project_that_is_no_repository_of_its_own_is_refused() {
    charter_core::unsteered!();
    // The project is itself a repository, so git answers for any folder in it. A folder of
    // the workspace that is not a repo's top is not a repo of the workspace.
    let f = support::plane_with_clone("thing");
    support::git(&f.plane, &["init", "-q", "."]);
    let fake = f.plane.join("workspaces/alpha/notarepo");
    std::fs::create_dir_all(&fake).unwrap();
    std::fs::write(fake.join("x.txt"), "x\n").unwrap();

    let refused = files::tree(&f.plane, Branch::repo(&f.ws, "notarepo"), "");

    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_refusal_never_says_worktree_piece_or_clone() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let said = [
        files::tree(&f.plane, Branch::piece(&f.ws, &f.repo, "nowhere"), "").unwrap_err(),
        files::tree(&f.plane, branch(&f), "..").unwrap_err(),
        files::tree(&f.plane, branch(&f), "gone").unwrap_err(),
        files::open(&f.plane, branch(&f), "gone.txt").unwrap_err(),
    ];

    for one in said {
        let one = one.to_string().to_lowercase();
        for word in ["worktree", "piece", "clone"] {
            assert!(!one.contains(word), "{one:?} says {word}");
        }
    }
}

#[cfg(unix)]
#[test]
fn expanding_a_folder_runs_no_program_its_repos_config_names() {
    charter_core::unsteered!();
    use std::os::unix::fs::PermissionsExt;
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let marker = f.plane.join("RAN");
    let program = f.plane.join("monitor.sh");
    std::fs::write(&program, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    support::git(
        &piece,
        &["config", "core.fsmonitor", &program.display().to_string()],
    );

    files::tree(&f.plane, branch(&f), "").unwrap();

    assert!(
        !marker.exists(),
        "the fsmonitor the repo's config names ran"
    );
}

#[cfg(unix)]
#[test]
fn a_name_that_starts_like_a_pathspec_is_asked_about_as_a_name() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join(":(glob)*"), "x\n").unwrap();
    std::fs::write(piece.join(".gitignore"), "*.tmp\n").unwrap();
    std::fs::write(piece.join("a.tmp"), "x\n").unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;

    assert!(!entry(&top, ":(glob)*").ignored);
    assert!(entry(&top, "a.tmp").ignored);
}

/// A name spelled with a combining accent, as macOS's file systems keep it: what git composes
/// into one character when it lists the folder.
const DECOMPOSED: &str = "se\u{301}cret.env";

#[test]
fn an_ignored_file_whose_name_git_spells_differently_is_still_ignored_and_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    // The last rule names its file composed, as a person types it; the file is decomposed.
    std::fs::write(piece.join(".gitignore"), "*.env\nn\u{e9}e.key\n").unwrap();
    std::fs::write(piece.join(DECOMPOSED), "API_TOKEN=x\n").unwrap();
    std::fs::write(piece.join("ne\u{301}e.key"), "API_TOKEN=x\n").unwrap();
    std::fs::write(piece.join("q\"uote.env"), "API_TOKEN=x\n").unwrap();
    std::fs::write(piece.join("back\\slash.env"), "API_TOKEN=x\n").unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;

    // Where the file system keeps the name as it was written (Linux), git lists it so too, and
    // the composed rule does not name it: there the decomposed file is not git's to ignore.
    let composes = cfg!(target_os = "macos");
    let names: &[&str] = if composes {
        &[
            DECOMPOSED,
            "ne\u{301}e.key",
            "q\"uote.env",
            "back\\slash.env",
        ]
    } else {
        &[DECOMPOSED, "q\"uote.env", "back\\slash.env"]
    };
    for &name in names {
        let secret = entry(&top, name);
        assert!(
            secret.ignored && secret.refused.is_some(),
            "{name:?}: {secret:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_link_to_an_ignored_file_whose_name_git_spells_differently_is_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join(".gitignore"), "*.env\n").unwrap();
    std::fs::write(piece.join(DECOMPOSED), "API_TOKEN=x\n").unwrap();
    std::os::unix::fs::symlink(DECOMPOSED, piece.join("settings.txt")).unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;

    let settings = entry(&top, "settings.txt");
    assert!(
        settings
            .refused
            .as_deref()
            .is_some_and(|why| why.contains("ignore")),
        "{settings:?}"
    );
}

#[test]
fn a_folder_level_answers_its_first_entries_and_counts_the_rest() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let many = piece.join("many");
    std::fs::create_dir_all(many.join("a-folder")).unwrap();
    for n in 0..files::SHOWN + 2 {
        std::fs::write(many.join(format!("f{n}.txt")), "").unwrap();
    }

    let level = files::tree(&f.plane, branch(&f), "many").unwrap();

    assert_eq!(level.entries.len(), files::SHOWN);
    assert_eq!(level.more, 3);
    // Sorted before the cut: the folder first, then the files from the smallest number.
    assert_eq!(level.entries[0].name, "a-folder");
    assert_eq!(level.entries[1].name, "f0.txt");
    assert_eq!(level.entries[2].name, "f1.txt");
}

#[test]
fn several_folders_of_one_branch_resolve_together_and_only_up_to_the_cap() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = std::fs::canonicalize(cut(&f, "piece")).unwrap();
    std::fs::create_dir_all(piece.join("src")).unwrap();
    let mut named = vec!["", "src", "..", "gone"];
    named.extend(std::iter::repeat_n("src", files::WATCHED));

    let resolved = files::folders(&f.plane, branch(&f), &named);

    assert_eq!(resolved.len(), named.len());
    assert_eq!(resolved[0].as_ref().unwrap(), &piece);
    assert_eq!(resolved[1].as_ref().unwrap(), &piece.join("src"));
    assert!(resolved[2].is_err() && resolved[3].is_err());
    assert!(resolved[files::WATCHED].is_err(), "past the cap is refused");
}

#[cfg(unix)]
#[test]
fn a_submodule_never_checked_out_is_drawn_and_nothing_in_it_opens() {
    charter_core::unsteered!();
    // A submodule the index records but that was never checked out has no `.git` of its own on
    // disk; git refuses any question about a path inside it, so charter must not ask one.
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let head = support::git(&piece, &["rev-parse", "HEAD"]);
    let sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
    support::git(
        &piece,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{sha},sub"),
        ],
    );
    std::fs::create_dir_all(piece.join("sub")).unwrap();
    std::fs::write(piece.join("sub/f.txt"), "x\n").unwrap();
    std::os::unix::fs::symlink("sub/f.txt", piece.join("l.txt")).unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;
    let sub = files::tree(&f.plane, branch(&f), "sub").unwrap().entries;

    assert!(
        entry(&top, "l.txt").refused.is_some(),
        "{:?}",
        entry(&top, "l.txt")
    );
    assert!(entry(&top, "README.md").refused.is_none(), "{top:?}");
    let inside = entry(&sub, "f.txt");
    let why = inside.refused.as_deref().expect("nothing in it opens");
    assert!(!why.contains("fatal") && !why.contains("Pathspec"), "{why}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_link_to_a_file_whose_name_is_not_utf8_is_refused() {
    charter_core::unsteered!();
    use std::os::unix::ffi::OsStrExt;
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let odd = std::ffi::OsStr::from_bytes(b"caf\xe9.txt");
    std::fs::write(piece.join(odd), "x\n").unwrap();
    std::os::unix::fs::symlink(odd, piece.join("l.txt")).unwrap();

    let top = files::tree(&f.plane, branch(&f), "").unwrap().entries;

    assert!(entry(&top, "l.txt").refused.is_some(), "{top:?}");
}
