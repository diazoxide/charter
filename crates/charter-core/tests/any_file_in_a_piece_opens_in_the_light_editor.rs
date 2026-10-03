//! RC-5 (#706): **any file in a piece opens** in the light editor (ADR 0081 §1, ADR 0084 §3).
//!
//! The window asks for a piece by its workspace, repo and piece name, never by a directory,
//! and for a file by its path relative to that piece. What it gets back is the piece's file
//! list and one file's text, or a sentence saying why not.

mod support;

use charter_core::files::{self, Branch, Opened};
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

    let files = files::list(&f.plane, Branch::piece(&f.ws, &f.repo, "piece")).unwrap();

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

    let opened = files::open(
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "piece"),
        "src/lib.rs",
    )
    .unwrap();

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

    let opened = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), "logo.png").unwrap();

    assert_eq!(opened, Opened::Binary { bytes: 8 });
}

#[test]
fn a_file_past_the_light_editors_size_is_offered_to_your_editor_instead() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let big = files::LARGEST + 1;
    std::fs::write(piece.join("big.log"), vec![b'a'; big as usize]).unwrap();

    let opened = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), "big.log").unwrap();

    assert_eq!(opened, Opened::TooLarge { bytes: big });
}

/// FM-2 (#1105): the file tab's preview draws an image as an image. It is known by what its
/// first bytes say, never by its name, so a `.png` that is really text opens as text.
#[test]
fn an_image_opens_as_an_image_known_by_its_bytes() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let png = png_header(2, 1);
    // Start of image, then a baseline frame header: 8-bit, 1 high, 2 wide, three components.
    let jpeg = [
        &[0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8, 0, 1, 0, 2, 3][..],
        &[1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0],
    ]
    .concat();
    // Its logical screen descriptor: 1 by 1, no colour table.
    let gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00".to_vec();
    // A lossless WebP's header: its signature byte, then width and height less one, 14 bits each.
    let webp = [
        &b"RIFF"[..],
        &[26, 0, 0, 0],
        &b"WEBPVP8L"[..],
        &[5, 0, 0, 0, 0x2F, 0, 0, 0, 0],
        &[0; 8],
    ]
    .concat();
    let cases = [
        ("shot.png", png, "image/png"),
        ("photo.jpg", jpeg, "image/jpeg"),
        ("spinner.gif", gif, "image/gif"),
        ("art.webp", webp, "image/webp"),
        // Named for another kind: what is inside decides.
        ("misnamed.txt", png_header(3, 3), "image/png"),
    ];
    for (name, bytes, mime) in cases {
        std::fs::write(piece.join(name), &bytes).unwrap();

        let opened = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), name).unwrap();

        assert_eq!(opened, Opened::Image { mime, data: bytes }, "{name}");
    }
    // A name that says image does not make text one.
    std::fs::write(piece.join("fake.png"), "just text\n").unwrap();
    let opened = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), "fake.png").unwrap();
    assert_eq!(
        opened,
        Opened::Text {
            text: "just text\n".to_string()
        }
    );
}

/// A PNG's signature and header chunk, declaring `width` by `height`, and nothing else: a few
/// dozen bytes that ask a decoder for a canvas of any size.
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    // 8-bit greyscale, deflate, adaptive filtering, no interlace; then a CRC nobody checks here.
    png.extend_from_slice(&[8, 0, 0, 0, 0, 0, 0, 0, 0]);
    png
}

/// FM-2 review: the 2 MiB cap bounds the bytes, not what they decode to. An image whose header
/// declares more than 40 megapixels is said by its size and never handed to the window, however
/// few bytes it is; a header that says no size is not an image the preview draws.
#[test]
fn an_image_declaring_a_huge_canvas_is_said_by_its_size_and_not_drawn() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join("bomb.png"), png_header(16_000, 16_000)).unwrap();
    std::fs::write(piece.join("fits.png"), png_header(8_000, 5_000)).unwrap();
    std::fs::write(piece.join("cut.png"), &png_header(1, 1)[..12]).unwrap();
    let open =
        |name: &str| files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), name).unwrap();

    assert_eq!(
        open("bomb.png"),
        Opened::HugeImage {
            mime: "image/png",
            width: 16_000,
            height: 16_000
        }
    );
    assert!(
        matches!(
            open("fits.png"),
            Opened::Image {
                mime: "image/png",
                ..
            }
        ),
        "40 megapixels exactly is drawn"
    );
    assert_eq!(open("cut.png"), Opened::Binary { bytes: 12 });
}

/// FM-2 (#1105, V86 F4): the preview reads at most 2 MiB of a file. Up to it a file is drawn,
/// past it only its size is said, whether it is text or an image.
#[test]
fn the_preview_reads_up_to_two_mebibytes_and_no_further() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let two = 2 * 1024 * 1024;
    std::fs::write(piece.join("fits.log"), vec![b'a'; two]).unwrap();
    std::fs::write(piece.join("over.log"), vec![b'a'; two + 1]).unwrap();
    let mut big_png = b"\x89PNG\r\n\x1a\n".to_vec();
    big_png.resize(two + 1, 0);
    std::fs::write(piece.join("huge.png"), &big_png).unwrap();
    let open =
        |name: &str| files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), name).unwrap();

    assert!(matches!(open("fits.log"), Opened::Text { text } if text.len() == two));
    assert_eq!(
        open("over.log"),
        Opened::TooLarge {
            bytes: two as u64 + 1
        }
    );
    assert_eq!(
        open("huge.png"),
        Opened::TooLarge {
            bytes: two as u64 + 1
        }
    );
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
        let refused = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), path);
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

    let away = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), "away.txt");
    let inside = files::open(
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "piece"),
        "AGENTS.md",
    )
    .unwrap();

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

    let refused = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), ".git");

    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_piece_git_does_not_have_is_refused_by_name() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");

    let refused = files::list(&f.plane, Branch::piece(&f.ws, &f.repo, "nowhere")).unwrap_err();

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

    files::list(&f.plane, Branch::piece(&f.ws, &f.repo, "piece")).unwrap();

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

    let files = files::list(&f.plane, Branch::piece(&f.ws, &f.repo, "piece")).unwrap();
    let refused = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), ".env");

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

    let refused = files::open(
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "piece"),
        "settings.txt",
    );

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

    let refused = files::open(
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "piece"),
        "sub/.git/config",
    );

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

    let refused = files::open(
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "piece"),
        "docs/secret.txt",
    );

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

    let refused = files::open(&f.plane, Branch::piece(&f.ws, &f.repo, "piece"), "pipe");

    assert!(refused.is_err(), "{refused:?}");
}
