//! FM-7 (#1110): **⌘P finds a file by fuzzy name** (#1103, V86 F10). The names come from the
//! branch's offered list — what git tracks, and what it does not track and does not ignore —
//! so what the light editor would refuse to open is never found either.

mod support;

use charter_core::files::{self, Branch, Finder, Place};
use charter_core::worktree;

fn cut(f: &support::Fixture, piece: &str) -> std::path::PathBuf {
    worktree::add(&f.plane, &f.ws, &f.repo, piece, None)
        .expect("a piece is cut")
        .path
}

fn write(at: &std::path::Path, path: &str) {
    let file = at.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, "\n").unwrap();
}

/// The paths found for `query` in one branch, best first.
fn found(f: &support::Fixture, piece: &str, query: &str) -> Vec<String> {
    let scope = [Place {
        plane: &f.plane,
        branch: Branch::piece(&f.ws, &f.repo, piece),
    }];
    let answer = files::find(&scope, query, 50);
    assert!(answer.refused.is_empty(), "{:?}", answer.refused);
    answer.hits.into_iter().map(|hit| hit.path).collect()
}

#[test]
fn a_fuzzy_fragment_ranks_the_file_it_means_first() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    for path in [
        "crates/core/src/files/find.rs",
        "crates/core/src/files.rs",
        "crates/core/src/finder_notes.md",
        "app/src/Palette.tsx",
        "app/src/fileFind.ts",
        "docs/find-a-file.md",
    ] {
        write(&piece, path);
    }

    let hits = found(&f, "piece", "fil/find");

    assert_eq!(
        hits.first().map(String::as_str),
        Some("crates/core/src/files/find.rs"),
        "{hits:?}"
    );
    assert_eq!(
        found(&f, "piece", "pltsx").first().map(String::as_str),
        Some("app/src/Palette.tsx")
    );
    assert!(
        !hits.iter().any(|one| one == "app/src/Palette.tsx"),
        "a path without those letters in order is not found: {hits:?}"
    );
}

#[test]
fn ignored_files_and_a_secret_file_git_ignores_are_never_found() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join(".gitignore"), "target/\n.env\n").unwrap();
    write(&piece, "target/debug/config.rs");
    std::fs::write(piece.join(".env"), "API_TOKEN=sk-live-0123456789abcdef\n").unwrap();
    write(&piece, "src/config.rs");
    write(&piece, "src/env.rs");

    assert_eq!(found(&f, "piece", "config"), ["src/config.rs"]);
    assert_eq!(found(&f, "piece", "env"), ["src/env.rs"]);
    assert!(
        found(&f, "piece", "head").is_empty(),
        "nothing of git's own folder is found"
    );
}

#[test]
fn a_link_is_found_only_when_it_leads_to_another_file_of_the_branch() {
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
    std::os::unix::fs::symlink("..", piece.join("up.txt")).unwrap();

    assert!(found(&f, "piece", "away").is_empty());
    assert!(found(&f, "piece", "settings").is_empty());
    assert!(found(&f, "piece", "up.txt").is_empty());
    assert_eq!(found(&f, "piece", "agents"), ["AGENTS.md"]);
}

#[test]
fn nothing_inside_a_submodule_or_a_nested_repository_is_found() {
    charter_core::unsteered!();
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
    write(&piece, "sub/inner.txt");
    std::os::unix::fs::symlink("sub/inner.txt", piece.join("innerlink.txt")).unwrap();
    let nested = piece.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    support::git(&nested, &["init", "-q", "-b", "main", "."]);
    write(&nested, "deep.txt");

    assert!(
        found(&f, "piece", "inner").is_empty(),
        "{:?}",
        found(&f, "piece", "inner")
    );
    assert!(found(&f, "piece", "sub").is_empty());
    assert!(found(&f, "piece", "deep").is_empty());
    assert!(found(&f, "piece", "nested").is_empty());
}

#[test]
fn a_scope_of_several_branches_says_which_branch_each_hit_is_in_nearest_first() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let one = cut(&f, "one");
    let two = cut(&f, "two");
    write(&one, "notes/plan.md");
    write(&two, "notes/plan.md");
    let scope = [
        Place {
            plane: &f.plane,
            branch: Branch::piece(&f.ws, &f.repo, "two"),
        },
        Place {
            plane: &f.plane,
            branch: Branch::piece(&f.ws, &f.repo, "one"),
        },
    ];

    let answer = files::find(&scope, "plan", 50);

    let hits: Vec<(usize, &str)> = answer
        .hits
        .iter()
        .map(|hit| (hit.at, hit.path.as_str()))
        .collect();
    assert_eq!(hits, [(0, "notes/plan.md"), (1, "notes/plan.md")]);
}

#[test]
fn a_projects_branches_are_its_repos_own_folders_and_their_pieces() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "one");
    let other = f.workspace().join("other");
    std::fs::create_dir_all(&other).unwrap();
    support::git(&other, &["init", "-q", "-b", "main", "."]);

    let named: Vec<(String, String, Option<String>)> = files::branches(&f.plane)
        .into_iter()
        .map(|one| (one.ws, one.repo, one.piece))
        .collect();

    let ws = || f.ws.clone();
    assert_eq!(
        named,
        [
            (ws(), "other".to_string(), None),
            (ws(), "thing".to_string(), None),
            (ws(), "thing".to_string(), Some("one".to_string())),
        ]
    );
}

#[test]
fn a_finder_lists_each_branch_once_for_its_session() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "first.md");
    let scope = [Place {
        plane: &f.plane,
        branch: Branch::piece(&f.ws, &f.repo, "piece"),
    }];
    let mut session = Finder::default();
    // What the window asks as the palette opens: nothing found, the scope listed.
    assert!(session.find(&scope, "", 50).hits.is_empty());

    write(&piece, "second.md");

    assert_eq!(session.find(&scope, "first", 50).hits.len(), 1);

    assert!(
        session.find(&scope, "second", 50).hits.is_empty(),
        "the session's listing is the one it read"
    );
    assert_eq!(
        files::find(&scope, "second", 50).hits.len(),
        1,
        "a new session lists again"
    );
}

#[test]
fn a_branch_that_cannot_be_listed_is_said_and_the_rest_still_answer() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "kept.md");
    let scope = [
        Place {
            plane: &f.plane,
            branch: Branch::piece(&f.ws, &f.repo, "gone"),
        },
        Place {
            plane: &f.plane,
            branch: Branch::piece(&f.ws, &f.repo, "piece"),
        },
    ];

    let answer = files::find(&scope, "kept", 50);

    assert_eq!(answer.hits.len(), 1);
    assert_eq!(answer.hits[0].at, 1);
    assert_eq!(answer.refused.len(), 1);
    assert_eq!(answer.refused[0].0, 0);
    assert!(answer.refused[0].1.contains("gone"), "{:?}", answer.refused);
    assert!(
        !answer.refused[0].1.contains("worktree"),
        "{:?}",
        answer.refused
    );
}

#[test]
fn a_branch_past_the_listing_cap_is_searched_up_to_it_and_says_so() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    for name in ["a.md", "b.md", "c.md", "d.md"] {
        write(&piece, name);
    }
    let scope = [Place {
        plane: &f.plane,
        branch: Branch::piece(&f.ws, &f.repo, "piece"),
    }];
    let mut session = Finder::listing_at_most(3);

    let answer = session.find(&scope, "md", 50);

    // `README.md` is the fixture's own: five files, sorted, and the first three listed.
    let paths: Vec<&str> = answer.hits.iter().map(|hit| hit.path.as_str()).collect();
    assert_eq!(paths.len(), 3, "{paths:?}");
    assert!(
        paths
            .iter()
            .all(|one| ["README.md", "a.md", "b.md"].contains(one)),
        "{paths:?}"
    );
    assert_eq!(answer.partial.len(), 1, "{:?}", answer.partial);
    let (at, said) = &answer.partial[0];
    assert_eq!(*at, 0);
    assert!(
        said.contains("piece") && said.contains('5') && said.contains('3'),
        "{said}"
    );
    assert!(!said.contains("worktree"), "{said}");
}

#[test]
fn the_best_hits_are_kept_when_far_more_files_match_than_are_shown() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    for n in 0..300 {
        write(&piece, &format!("deep/folder/notes{n}.txt"));
    }
    write(&piece, "notes.txt");
    let scope = [Place {
        plane: &f.plane,
        branch: Branch::piece(&f.ws, &f.repo, "piece"),
    }];

    let hits = files::find(&scope, "notes", 5).hits;

    assert_eq!(hits.len(), 5);
    assert_eq!(hits[0].path, "notes.txt", "{hits:?}");
}

#[test]
fn an_empty_query_finds_nothing_and_the_most_is_kept() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    for n in 0..30 {
        write(&piece, &format!("page{n}.md"));
    }
    let scope = [Place {
        plane: &f.plane,
        branch: Branch::piece(&f.ws, &f.repo, "piece"),
    }];

    assert!(files::find(&scope, "  ", 50).hits.is_empty());
    assert_eq!(files::find(&scope, "page", 10).hits.len(), 10);
}

/// ⌘P's core half, measured by hand on a branch of 100,000 tracked files (FM-7): the listing
/// once per session, and each keystroke's core match, which is the part of ADR 0086 L1's 50 ms
/// (keystroke to screen) this crate spends. The window's half, the IPC and the draw, is
/// FM-12's to measure (#1115). Ignored by
/// default because the fixture takes a minute to build; run it with
/// `cargo test --release -p charter-core --test a_file_is_found_by_fuzzy_name -- --ignored
/// --nocapture`.
#[test]
#[ignore = "builds a 100,000-file fixture; run by hand to measure"]
fn measured_on_a_hundred_thousand_files() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    for d in 0..1000 {
        let dir = piece.join(format!("pkg{}/mod{d}", d % 37));
        std::fs::create_dir_all(&dir).unwrap();
        for n in 0..100 {
            std::fs::write(dir.join(format!("file_{n}_part.rs")), "\n").unwrap();
        }
    }
    support::git(&piece, &["add", "-A"]);
    support::git(&piece, &["commit", "-q", "-m", "many"]);
    let scope = [Place {
        plane: &f.plane,
        branch: Branch::piece(&f.ws, &f.repo, "piece"),
    }];
    let mut session = Finder::default();

    let first = std::time::Instant::now();
    let answer = session.find(&scope, "f", 50);
    let listed = first.elapsed();
    assert_eq!(answer.hits.len(), 50);

    let mut worst = std::time::Duration::ZERO;
    for query in [
        "m",
        "mo",
        "mod",
        "mod7",
        "mod77",
        "mod777",
        "mod777/f",
        "mod777/file_42",
    ] {
        let one = std::time::Instant::now();
        let answer = session.find(&scope, query, 50);
        worst = worst.max(one.elapsed());
        assert!(!answer.hits.is_empty(), "{query}");
    }
    let best = session.find(&scope, "mod777/file_42", 50);
    assert_eq!(best.hits[0].path, "pkg0/mod777/file_42_part.rs");
    println!("FM-7: first find (list + match) {listed:?}; worst core match after {worst:?}");
    assert!(worst.as_millis() <= 50, "within L1's 50 ms: {worst:?}");
}
