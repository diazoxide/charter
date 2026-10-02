//! RC-5 (#706): **any file in a piece opens** in the light editor (ADR 0081 §1, ADR 0084 §3).
//!
//! The window asks for a piece by its workspace, repo and piece name, never by a directory,
//! and for a file by its path relative to that piece. What it gets back is the piece's file
//! list and one file's text, or a sentence saying why not.

mod support;

use charter_core::piecefiles::{self, Opened};
use charter_core::worktree;

fn cut(f: &support::Fixture, piece: &str) -> std::path::PathBuf {
    worktree::add(&f.plane, &f.ws, &f.repo, piece, None)
        .expect("a piece is cut")
        .path
}

#[test]
fn a_pieces_files_are_the_ones_git_tracks_and_the_ones_it_does_not_ignore() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::create_dir_all(piece.join("src")).unwrap();
    std::fs::write(piece.join("src/new.rs"), "fn main() {}\n").unwrap();
    std::fs::write(piece.join(".gitignore"), "target/\n").unwrap();
    std::fs::create_dir_all(piece.join("target")).unwrap();
    std::fs::write(piece.join("target/out.bin"), "built\n").unwrap();

    let files = piecefiles::list(&f.plane, &f.ws, &f.repo, "piece").unwrap();

    assert!(files.contains(&"README.md".to_string()), "{files:?}");
    assert!(files.contains(&"src/new.rs".to_string()), "{files:?}");
    assert!(files.contains(&".gitignore".to_string()), "{files:?}");
    assert!(
        !files.iter().any(|one| one.starts_with("target/")),
        "an ignored file is not offered: {files:?}"
    );
}

#[test]
fn a_text_file_opens_with_its_contents() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::create_dir_all(piece.join("src")).unwrap();
    std::fs::write(piece.join("src/lib.rs"), "pub fn one() -> u8 {\n    1\n}\n").unwrap();

    let opened = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "src/lib.rs").unwrap();

    assert_eq!(
        opened,
        Opened::Text {
            text: "pub fn one() -> u8 {\n    1\n}\n".to_string()
        }
    );
}

#[test]
fn a_binary_file_says_so_and_is_not_drawn_as_text() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(
        piece.join("logo.png"),
        [0x89, b'P', b'N', b'G', 0, 0, 0, 13],
    )
    .unwrap();

    let opened = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "logo.png").unwrap();

    assert_eq!(opened, Opened::Binary { bytes: 8 });
}

#[test]
fn a_file_past_the_light_editors_size_is_offered_to_your_editor_instead() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let big = piecefiles::LARGEST + 1;
    std::fs::write(piece.join("big.log"), vec![b'a'; big as usize]).unwrap();

    let opened = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "big.log").unwrap();

    assert_eq!(opened, Opened::TooLarge { bytes: big });
}

#[test]
fn a_path_that_leaves_the_piece_is_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");
    std::fs::write(f.plane.join("charter.local.toml"), "secret = 1\n").unwrap();

    for path in [
        "../../../../charter.local.toml",
        "/etc/hosts",
        "src/../../README.md",
        "",
    ] {
        let refused = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", path);
        assert!(refused.is_err(), "{path:?} opened: {refused:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_link_is_followed_only_while_it_stays_inside_the_piece() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let outside = f.plane.join("outside.txt");
    std::fs::write(&outside, "not the piece's\n").unwrap();
    std::os::unix::fs::symlink(&outside, piece.join("away.txt")).unwrap();
    // A repo that links one instruction file to another, as many link CLAUDE.md to AGENTS.md.
    std::os::unix::fs::symlink("README.md", piece.join("AGENTS.md")).unwrap();

    let away = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "away.txt");
    let inside = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "AGENTS.md").unwrap();

    assert!(
        away.is_err(),
        "a link out of the piece was followed: {away:?}"
    );
    assert!(
        matches!(inside, Opened::Text { .. }),
        "a link to a file of the same piece opens: {inside:?}"
    );
}

#[test]
fn the_git_directory_is_not_a_file_of_the_piece() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let refused = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", ".git");

    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_piece_git_does_not_have_is_refused_by_name() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");

    let refused = piecefiles::list(&f.plane, &f.ws, &f.repo, "nowhere").unwrap_err();

    assert!(refused.to_string().contains("nowhere"), "{refused}");
}

#[cfg(unix)]
#[test]
fn listing_a_piece_runs_no_program_its_repos_config_names() {
    charter_core::unsteered!();
    use std::os::unix::fs::PermissionsExt;
    // ADR 0084 §2: every repo the light editor reads is one an agent can write, and git runs
    // a file-system monitor its config names on the verbs that read the working tree.
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

    piecefiles::list(&f.plane, &f.ws, &f.repo, "piece").unwrap();

    assert!(
        !marker.exists(),
        "the fsmonitor the repo's config names ran"
    );
}

#[test]
fn an_ignored_file_is_not_offered_and_does_not_open_by_name() {
    charter_core::unsteered!();
    // ADR 0084 §2's "what a review can show", and ADR 0052's line: what opens is what the
    // file list offers. An ignored `.env` holds the kind of value the vault keeps out of the
    // window.
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(piece.join(".env"), "API_TOKEN=sk-live-0123456789abcdef\n").unwrap();

    let files = piecefiles::list(&f.plane, &f.ws, &f.repo, "piece").unwrap();
    let refused = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", ".env");

    assert!(!files.contains(&".env".to_string()), "{files:?}");
    let said = refused.expect_err("an ignored file opened").to_string();
    assert!(said.contains(".env"), "{said}");
    assert!(
        !said.contains("sk-live"),
        "the refusal carries the value: {said}"
    );
}

#[cfg(unix)]
#[test]
fn a_link_to_an_ignored_file_does_not_open_it() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(piece.join(".env"), "API_TOKEN=sk-live-0123456789abcdef\n").unwrap();
    std::os::unix::fs::symlink(".env", piece.join("settings.txt")).unwrap();

    let refused = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "settings.txt");

    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_git_directory_anywhere_in_the_path_is_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    // A nested repo the worktree does not track: its `.git` is git's, not the branch's.
    let nested = piece.join("sub");
    std::fs::create_dir_all(&nested).unwrap();
    support::git(&nested, &["init", "-q", "."]);
    std::fs::write(nested.join("note.txt"), "hello\n").unwrap();

    let refused = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "sub/.git/config");

    assert!(refused.is_err(), "{refused:?}");
}

#[cfg(unix)]
#[test]
fn a_linked_directory_that_leaves_the_piece_is_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let outside = f.plane.join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), "not the piece's\n").unwrap();
    std::os::unix::fs::symlink(&outside, piece.join("docs")).unwrap();

    let refused = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "docs/secret.txt");

    assert!(refused.is_err(), "{refused:?}");
}

#[cfg(unix)]
#[test]
fn a_fifo_is_not_a_file_and_does_not_hang_the_open() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let made = charter_core::forklock::output(
        std::process::Command::new("mkfifo").arg(piece.join("pipe")),
    )
    .unwrap();
    assert!(made.status.success(), "{made:?}");

    let refused = piecefiles::open(&f.plane, &f.ws, &f.repo, "piece", "pipe");

    assert!(refused.is_err(), "{refused:?}");
}
