//! FM-8 (#1111): **⌘⇧F searches the content of a branch's files** (#1103, V86 F9). One core
//! engine on ripgrep's crates, a live scan with no index: literal or regex, match case, whole
//! word. What it reads is what the light editor opens, and never less guarded: no ignored file,
//! no link, nothing in a vault or named like a credential, no binary file.

mod support;

use std::sync::atomic::AtomicBool;

use charter_core::files::{self, Branch, Ended, FileHits, Place, SearchOptions, Searched};
use charter_core::worktree;

fn cut(f: &support::Fixture, piece: &str) -> std::path::PathBuf {
    worktree::add(&f.plane, &f.ws, &f.repo, piece, None)
        .expect("a piece is cut")
        .path
}

fn write(at: &std::path::Path, path: &str, text: &str) {
    let file = at.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn place<'a>(f: &'a support::Fixture, piece: &'a str) -> Place<'a> {
    Place {
        plane: &f.plane,
        branch: Branch::piece(&f.ws, &f.repo, piece),
    }
}

/// Everything a search of `scope` heard, run to its end in pages of `page` lines.
fn heard(scope: &[Place<'_>], query: &str, options: SearchOptions) -> Vec<FileHits> {
    let mut search = files::search(scope, query, options).expect("the query compiles");
    let mut files = Vec::new();
    let stop = AtomicBool::new(false);
    loop {
        let ended = search.more(1000, &stop, &mut |step| match step {
            Searched::File(hits) => files.push(hits),
            Searched::Refused { why, .. } => panic!("refused: {why}"),
            Searched::NotSearched { path, why, .. } => panic!("{path} not searched: {why}"),
        });
        if ended == Ended::Done {
            return files;
        }
    }
}

/// Each hit as `path:line: text`, in the order heard.
fn lines(files: &[FileHits]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            file.lines
                .iter()
                .map(move |line| format!("{}:{}: {}", file.path, line.number, line.text()))
        })
        .collect()
}

fn literal() -> SearchOptions {
    SearchOptions::default()
}

#[test]
fn a_literal_query_finds_each_line_holding_it_with_its_number() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(
        &piece,
        "src/login.rs",
        "fn main() {}\nlet token = a.b(); // a.b\nnone\n",
    );
    write(&piece, "docs/guide.md", "call a.b first\n");
    write(&piece, "src/other.rs", "axb is not it\n");

    let files = heard(&[place(&f, "piece")], "a.b", literal());

    assert_eq!(
        lines(&files),
        [
            "docs/guide.md:1: call a.b first",
            "src/login.rs:2: let token = a.b(); // a.b",
        ],
        "a literal's dot is a dot, not any character"
    );
    let login = files.iter().find(|one| one.path == "src/login.rs").unwrap();
    assert_eq!(login.count, 1, "one matching line");
    let hit: Vec<(&str, bool)> = login.lines[0]
        .parts
        .iter()
        .map(|part| (part.text.as_str(), part.hit))
        .collect();
    assert_eq!(
        hit,
        [
            ("let token = ", false),
            ("a.b", true),
            ("(); // ", false),
            ("a.b", true)
        ]
    );
}

#[test]
fn a_regex_matches_as_a_pattern_and_a_bad_one_is_refused_with_its_reason() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(
        &piece,
        "src/a.rs",
        "fn open_file()\nfn close()\nfn openly\n",
    );
    let regex = SearchOptions {
        regex: true,
        ..literal()
    };

    let files = heard(&[place(&f, "piece")], r"fn open\w*\(\)", regex);

    assert_eq!(lines(&files), ["src/a.rs:1: fn open_file()"]);
    assert!(
        heard(&[place(&f, "piece")], r"fn open\w*\(\)", literal()).is_empty(),
        "the same text as a literal is looked for as written"
    );
    let bad = files::search(&[place(&f, "piece")], "open(", regex);
    assert!(
        matches!(bad, Err(files::BadQuery::Pattern(ref why)) if why.contains("unclosed")),
        "{:?}",
        bad.err()
    );
}

#[test]
fn case_is_ignored_unless_asked_to_match() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "notes.txt", "Token\ntoken\nTOKEN\n");

    let any = heard(&[place(&f, "piece")], "token", literal());
    let exact = heard(
        &[place(&f, "piece")],
        "Token",
        SearchOptions {
            match_case: true,
            ..literal()
        },
    );

    assert_eq!(
        lines(&any),
        [
            "notes.txt:1: Token",
            "notes.txt:2: token",
            "notes.txt:3: TOKEN"
        ]
    );
    assert_eq!(lines(&exact), ["notes.txt:1: Token"]);
}

#[test]
fn a_whole_word_match_skips_the_word_inside_another() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(
        &piece,
        "src/a.rs",
        "let id = 1;\nlet idle = 2;\nlet my_id = 3;\n(id)\n",
    );

    let words = heard(
        &[place(&f, "piece")],
        "id",
        SearchOptions {
            whole_word: true,
            ..literal()
        },
    );

    assert_eq!(
        lines(&words),
        ["src/a.rs:1: let id = 1;", "src/a.rs:4: (id)"]
    );
    let marked: Vec<&str> = words[0].lines[1]
        .parts
        .iter()
        .filter(|part| part.hit)
        .map(|part| part.text.as_str())
        .collect();
    assert_eq!(
        marked,
        ["id"],
        "only the word is marked, not what bounds it"
    );
}

#[test]
fn ignored_files_and_folders_are_never_read() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join(".gitignore"), "target/\n*.log\n").unwrap();
    write(&piece, "target/debug/build.rs", "needle\n");
    write(&piece, "run.log", "needle\n");
    write(&piece, "src/kept.rs", "needle\n");

    let files = heard(&[place(&f, "piece")], "needle", literal());

    assert_eq!(lines(&files), ["src/kept.rs:1: needle"]);
}

#[test]
fn a_file_named_like_a_credential_or_in_a_vault_is_never_read_even_where_git_offers_it() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    // None of these is ignored: an agent could have committed every one.
    for path in [
        ".env",
        ".env.production",
        "deploy/id_ed25519",
        "certs/server.pem",
        "config/credentials.json",
        ".npmrc",
        ".charter/vaults/main.json",
        "tools/.charter/browser/state.json",
    ] {
        write(&piece, path, "needle = sk-live-0123456789abcdef\n");
    }
    write(&piece, ".env.example", "needle = put yours here\n");
    support::git(&piece, &["add", "-A"]);

    let files = heard(&[place(&f, "piece")], "needle", literal());

    assert_eq!(lines(&files), [".env.example:1: needle = put yours here"]);
}

#[cfg(unix)]
#[test]
fn a_link_is_never_followed_into_or_out_of_the_branch() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let outside = f.plane.join("elsewhere");
    write(&outside, "secret.txt", "needle outside\n");
    write(&piece, "src/real.rs", "needle inside\n");
    std::os::unix::fs::symlink(outside.join("secret.txt"), piece.join("out.txt")).unwrap();
    std::os::unix::fs::symlink(&outside, piece.join("away")).unwrap();
    std::os::unix::fs::symlink("src/real.rs", piece.join("again.rs")).unwrap();
    support::git(&piece, &["add", "-A"]);

    let files = heard(&[place(&f, "piece")], "needle", literal());

    assert_eq!(
        lines(&files),
        ["src/real.rs:1: needle inside"],
        "a link, even to a file of the branch, is the file's, searched once where it is"
    );
}

#[test]
fn a_binary_file_or_an_image_or_one_past_the_preview_is_never_read() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join("blob.bin"), b"needle\0\x01\x02").unwrap();
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(b"\nneedle\n");
    std::fs::write(piece.join("pic.png"), png).unwrap();
    let mut big = "needle\n".repeat(10);
    big.push_str(&"x".repeat(files::LARGEST as usize));
    std::fs::write(piece.join("huge.txt"), big).unwrap();
    write(&piece, "small.txt", "needle\n");

    let files = heard(&[place(&f, "piece")], "needle", literal());

    assert_eq!(lines(&files), ["small.txt:1: needle"]);
}

#[test]
fn gits_own_folder_and_a_repository_nested_in_the_branch_are_never_read() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    // The repo's own folder, where `.git` is a folder.
    write(&f.clone, ".git/needle.txt", "needle\n");
    write(&f.clone, "vendor/lib/.git/HEAD", "ref: refs/heads/main\n");
    write(&f.clone, "vendor/lib/inner.rs", "needle\n");
    write(&f.clone, "src/kept.rs", "needle\n");
    let scope = [Place {
        plane: &f.plane,
        branch: Branch::repo(&f.ws, &f.repo),
    }];

    let files = heard(&scope, "needle", literal());

    assert_eq!(lines(&files), ["src/kept.rs:1: needle"]);
}

#[test]
fn hits_stream_a_file_at_a_time_stop_at_the_page_and_more_continues_where_it_stopped() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    for n in 0..5 {
        write(&piece, &format!("src/f{n}.rs"), "needle\nneedle\n");
    }
    let mut search = files::search(&[place(&f, "piece")], "needle", literal()).unwrap();
    let stop = AtomicBool::new(false);
    let mut pages: Vec<Vec<String>> = Vec::new();

    let mut ends = Vec::new();
    loop {
        let mut page = Vec::new();
        let ended = search.more(3, &stop, &mut |step| {
            if let Searched::File(file) = step {
                page.push(file.path);
            }
        });
        pages.push(page);
        ends.push(ended);
        if ended == Ended::Done {
            break;
        }
    }

    // Three lines a page, counted by whole files: two files (four lines) a page.
    assert_eq!(
        pages,
        [
            vec!["src/f0.rs", "src/f1.rs"],
            vec!["src/f2.rs", "src/f3.rs"],
            vec!["src/f4.rs"],
        ]
    );
    assert_eq!(ends, [Ended::Capped, Ended::Capped, Ended::Done]);
}

/// #1153: files are read on several threads, a page ahead of what is heard. What is heard is
/// still every file once, in walk order, page after page, whatever the readers finished first.
#[test]
fn files_read_on_several_threads_are_heard_once_each_in_walk_order_page_after_page() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let mut expected = Vec::new();
    for folder in 0..12 {
        for file in 0..25 {
            let path = format!("d{folder}/f{file}.txt");
            // Some files large, so the readers finish out of order.
            let text = if file % 7 == 0 {
                format!("{}needle\n", "filler line\n".repeat(40_000))
            } else if file % 3 == 0 {
                "nothing here\n".to_string()
            } else {
                "needle\n".to_string()
            };
            if file % 3 != 0 || file % 7 == 0 {
                expected.push(path.clone());
            }
            write(&piece, &path, &text);
        }
    }
    expected.sort();
    let mut search = files::search(&[place(&f, "piece")], "needle", literal()).unwrap();
    let stop = AtomicBool::new(false);
    let mut heard = Vec::new();
    let mut pages = 0;

    loop {
        pages += 1;
        let ended = search.more(7, &stop, &mut |step| match step {
            Searched::File(file) => heard.push(file.path),
            other => panic!("{other:?}"),
        });
        if ended == Ended::Done {
            break;
        }
        assert_eq!(ended, Ended::Capped);
    }

    assert_eq!(heard, expected);
    assert!(pages > 20, "{pages} pages");
}

/// A stop raised mid-page loses nothing: a page asked after it goes on from the first file not
/// yet heard, though the readers had read past it.
#[test]
fn a_page_after_a_stop_goes_on_from_the_first_file_not_heard() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let mut expected = Vec::new();
    for n in 0..200 {
        let path = format!("f{n:03}.txt");
        write(&piece, &path, "needle\n");
        expected.push(path);
    }
    let mut search = files::search(&[place(&f, "piece")], "needle", literal()).unwrap();
    let stop = AtomicBool::new(false);
    let mut heard = Vec::new();

    let ended = search.more(1000, &stop, &mut |step| {
        if let Searched::File(file) = step {
            heard.push(file.path);
        }
        if heard.len() == 10 {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    assert_eq!(ended, Ended::Stopped);
    assert_eq!(heard.len(), 10);
    let stop = AtomicBool::new(false);
    let ended = search.more(1000, &stop, &mut |step| {
        if let Searched::File(file) = step {
            heard.push(file.path);
        }
    });

    assert_eq!(ended, Ended::Done);
    assert_eq!(heard, expected);
}

#[test]
fn a_raised_stop_ends_the_search_before_another_file_is_read() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    for n in 0..5 {
        write(&piece, &format!("f{n}.txt"), "needle\n");
    }
    let mut search = files::search(&[place(&f, "piece")], "needle", literal()).unwrap();
    let stop = AtomicBool::new(false);
    let mut heard = 0;

    let ended = search.more(1000, &stop, &mut |_| {
        heard += 1;
        // The window started a new search: this one is told to stop.
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    assert_eq!(ended, Ended::Stopped);
    assert_eq!(heard, 1);
}

#[test]
fn a_page_out_of_time_says_so_and_more_continues() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "a.txt", "needle\n");
    let mut search = files::search(&[place(&f, "piece")], "needle", literal())
        .unwrap()
        .with_page_time(std::time::Duration::ZERO);
    let stop = AtomicBool::new(false);

    let ended = search.more(1000, &stop, &mut |_| panic!("no time to read anything"));

    assert_eq!(ended, Ended::OutOfTime);
    let mut search = search.with_page_time(files::PAGE_TIME);
    let mut found = Vec::new();
    assert_eq!(
        search.more(1000, &stop, &mut |step| found.push(step)),
        Ended::Done
    );
    assert_eq!(found.len(), 1);
}

#[test]
fn a_file_with_many_matches_shows_its_first_lines_and_counts_them_all() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(
        &piece,
        "many.txt",
        &"needle\n".repeat(files::FILE_LINES + 7),
    );
    let long = format!("{}needle{}\n", "a".repeat(5000), "b".repeat(5000));
    write(&piece, "long.txt", &long);

    let files = heard(&[place(&f, "piece")], "needle", literal());

    let many = files.iter().find(|one| one.path == "many.txt").unwrap();
    assert_eq!(many.count, (files::FILE_LINES + 7) as u64);
    assert_eq!(many.lines.len(), files::FILE_LINES);
    let long = &files
        .iter()
        .find(|one| one.path == "long.txt")
        .unwrap()
        .lines[0];
    assert!(long.clipped);
    assert!(
        long.text().chars().count() <= files::LINE_CHARS,
        "{}",
        long.text().len()
    );
    assert!(
        long.parts
            .iter()
            .any(|part| part.hit && part.text == "needle")
    );
}

#[test]
fn a_scope_of_several_projects_searches_each_place_in_order_and_nothing_else() {
    charter_core::unsteered!();
    let one = support::plane_with_clone("thing");
    let two = support::plane_with_clone("thing");
    let left_out = support::plane_with_clone("thing");
    for f in [&one, &two, &left_out] {
        let piece = cut(f, "piece");
        write(&piece, "src/a.rs", "needle\n");
    }
    // Every branch of the two projects in the scope: each repo's own folder, and its piece.
    let mut scope = Vec::new();
    let named: Vec<(&std::path::Path, Vec<files::Named>)> = [&two, &one]
        .iter()
        .map(|f| (f.plane.as_path(), files::branches(&f.plane)))
        .collect();
    for (plane, branches) in &named {
        for branch in branches {
            scope.push(Place {
                plane,
                branch: branch.branch(),
            });
        }
    }

    let files = heard(&scope, "needle", literal());

    let found: Vec<(usize, &str)> = files
        .iter()
        .map(|file| (file.at, file.path.as_str()))
        .collect();
    // The pieces only: the repos' own folders hold README.md and no needle.
    assert_eq!(found, [(1, "src/a.rs"), (3, "src/a.rs")]);
    assert_eq!(scope[1].plane, two.plane.as_path());
    assert_eq!(scope[3].plane, one.plane.as_path());
}

#[test]
fn a_branch_that_cannot_be_read_is_said_and_the_rest_of_the_scope_still_searched() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "a.txt", "needle\n");
    let scope = [place(&f, "gone"), place(&f, "piece")];
    let mut search = files::search(&scope, "needle", literal()).unwrap();
    let stop = AtomicBool::new(false);
    let mut steps = Vec::new();

    assert_eq!(
        search.more(1000, &stop, &mut |step| steps.push(step)),
        Ended::Done
    );

    assert!(
        matches!(&steps[0], Searched::Refused { at: 0, why } if why.contains("gone")),
        "{steps:?}"
    );
    assert!(matches!(&steps[1], Searched::File(file) if file.at == 1));
}

#[test]
fn an_empty_or_multi_line_or_overlong_query_is_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let scope = [Place {
        plane: &f.plane,
        branch: Branch::repo(&f.ws, &f.repo),
    }];
    let refused = |query: &str| files::search(&scope, query, literal()).err();

    assert_eq!(refused(""), Some(files::BadQuery::Empty));
    assert_eq!(refused("a\nb"), Some(files::BadQuery::Lines));
    assert_eq!(
        refused(&"a".repeat(files::LONGEST_QUERY + 1)),
        Some(files::BadQuery::TooLong)
    );
    let huge = files::search(
        &scope,
        r"\w{1000}{1000}",
        SearchOptions {
            regex: true,
            ..literal()
        },
    );
    assert!(
        matches!(huge, Err(files::BadQuery::Pattern(_))),
        "a regex past the size limit is refused, not built"
    );
}

/// A stop raised `after` from another thread, and how long the page took to end.
fn stopped_after(
    search: &mut files::Search,
    after: std::time::Duration,
) -> (Ended, std::time::Duration, Vec<Searched>) {
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let raise = std::sync::Arc::clone(&stop);
    let raiser = std::thread::spawn(move || {
        std::thread::sleep(after);
        raise.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let started = std::time::Instant::now();
    let mut steps = Vec::new();
    let ended = search.more(usize::MAX, &stop, &mut |step| steps.push(step));
    let took = started.elapsed();
    raiser.join().unwrap();
    (ended, took, steps)
}

#[cfg(unix)]
#[test]
fn an_ignore_file_planted_as_a_link_to_a_device_or_a_fifo_is_never_opened() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "src/a.rs", "needle\n");
    std::fs::create_dir_all(piece.join("zero")).unwrap();
    std::os::unix::fs::symlink("/dev/zero", piece.join("zero/.gitignore")).unwrap();
    write(&piece, "zero/b.rs", "needle\n");
    std::fs::create_dir_all(piece.join("pipe")).unwrap();
    let fifo = f.plane.join("a-fifo");
    let made =
        charter_core::forklock::output(std::process::Command::new("mkfifo").arg(&fifo)).unwrap();
    assert!(made.status.success(), "{made:?}");
    std::os::unix::fs::symlink(&fifo, piece.join("pipe/.gitignore")).unwrap();
    write(&piece, "pipe/c.rs", "needle\n");
    let mut search = files::search(&[place(&f, "piece")], "needle", literal()).unwrap();

    let (ended, took, steps) = stopped_after(&mut search, std::time::Duration::from_secs(5));

    assert_eq!(
        ended,
        Ended::Done,
        "the page ends on its own, never blocked: {steps:?}"
    );
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
    let paths: Vec<&str> = steps
        .iter()
        .filter_map(|step| match step {
            Searched::File(file) => Some(file.path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(paths, ["pipe/c.rs", "src/a.rs", "zero/b.rs"]);
}

#[test]
fn a_tracked_file_under_an_ignore_pattern_is_searched_as_the_light_editor_opens_it() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "kept.log", "needle\n");
    support::git(&piece, &["add", "kept.log"]);
    std::fs::write(piece.join(".gitignore"), "*.log\n").unwrap();
    write(&piece, "other.log", "needle\n");

    let files = heard(&[place(&f, "piece")], "needle", literal());

    assert_eq!(lines(&files), ["kept.log:1: needle"]);
}

/// Text that makes [`SLOW`] slow: words and spaces outside ASCII and never a digit, in lines
/// of about `line` bytes, to `total` bytes. A Unicode word boundary on text that is not ASCII
/// is past what the lazy DFA does, so the matcher falls back to its slowest engine: some 8 s
/// for 2 MiB in a debug build, measured.
fn slow_text(line: usize, total: usize) -> String {
    let one = format!("{}\n", "лорем ипсум долор ".repeat(line / 30));
    one.repeat(total / one.len())
}

/// A pattern that compiles within the size limit and is slow on [`slow_text`].
const SLOW: &str = r"(?:\b\w+\b\s+){1,10}\d";

#[test]
fn a_raised_stop_ends_a_page_inside_a_file_that_is_slow_to_match() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(
        piece.join("slow.txt"),
        slow_text(1000, 2 * 1024 * 1024 - 4096),
    )
    .unwrap();
    let regex = SearchOptions {
        regex: true,
        ..literal()
    };
    let mut search = files::search(&[place(&f, "piece")], SLOW, regex).unwrap();

    let (ended, took, _) = stopped_after(&mut search, std::time::Duration::from_millis(500));

    assert_eq!(ended, Ended::Stopped);
    assert!(
        took < std::time::Duration::from_millis(1500),
        "stop was raised at 0.5 s and the page ended at {took:?}"
    );
}

#[test]
fn a_page_out_of_time_inside_a_slow_file_says_the_file_was_not_searched() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(
        piece.join("slow.txt"),
        slow_text(1000, 2 * 1024 * 1024 - 4096),
    )
    .unwrap();
    let regex = SearchOptions {
        regex: true,
        ..literal()
    };
    let mut search = files::search(&[place(&f, "piece")], SLOW, regex)
        .unwrap()
        .with_page_time(std::time::Duration::from_millis(500));
    let stop = AtomicBool::new(false);
    let mut steps = Vec::new();
    let started = std::time::Instant::now();

    let ended = search.more(usize::MAX, &stop, &mut |step| steps.push(step));

    assert_eq!(ended, Ended::OutOfTime);
    assert!(started.elapsed() < std::time::Duration::from_millis(1500));
    assert!(
        matches!(&steps[..], [Searched::NotSearched { path, why, .. }]
            if path == "slow.txt" && why.contains("longer than a page")),
        "{steps:?}"
    );
}

#[test]
fn a_file_with_a_line_past_the_longest_is_said_and_not_searched() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let long = format!("{}needle\n", "x".repeat(files::LONGEST_LINE + 10));
    write(&piece, "bundle.min.js", &long);
    write(&piece, "src/a.rs", "needle\n");
    let mut search = files::search(&[place(&f, "piece")], "needle", literal()).unwrap();
    let stop = AtomicBool::new(false);
    let mut steps = Vec::new();

    assert_eq!(
        search.more(1000, &stop, &mut |step| steps.push(step)),
        Ended::Done
    );

    assert!(
        matches!(&steps[0], Searched::NotSearched { path, why, .. }
            if path == "bundle.min.js" && why.contains("line longer than")),
        "{steps:?}"
    );
    assert!(matches!(&steps[1], Searched::File(file) if file.path == "src/a.rs"));
}

#[cfg(unix)]
#[test]
fn a_folder_swapped_for_a_link_while_the_search_is_paused_is_refused_not_followed() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "b/x1.txt", "needle inside\n");
    write(&piece, "b/x2.txt", "needle inside\n");
    let outside = f.plane.join("elsewhere");
    write(&outside, "x2.txt", "needle outside\n");
    let mut search = files::search(&[place(&f, "piece")], "needle", literal()).unwrap();
    let stop = AtomicBool::new(false);
    let mut first = Vec::new();
    // One line a page: the walk has listed `b/` and stops after `b/x1.txt`.
    assert_eq!(
        search.more(1, &stop, &mut |step| first.push(step)),
        Ended::Capped
    );
    assert!(matches!(&first[..], [Searched::File(file)] if file.path == "b/x1.txt"));

    std::fs::rename(piece.join("b"), piece.join("b-was")).unwrap();
    std::os::unix::fs::symlink(&outside, piece.join("b")).unwrap();
    let mut rest = Vec::new();
    let ended = search.more(1000, &stop, &mut |step| rest.push(step));

    assert_eq!(ended, Ended::Done);
    assert!(
        rest.iter()
            .all(|step| !matches!(step, Searched::File(file) if file.path == "b/x2.txt")),
        "the file behind the swapped folder was read: {rest:?}"
    );
}

#[cfg(unix)]
#[test]
fn an_ancestor_of_the_branch_folder_swapped_for_a_link_mid_search_is_never_followed() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    write(&piece, "b/x1.txt", "needle inside\n");
    write(&piece, "b/x2.txt", "needle inside\n");
    // An outside tree shaped like `.worktrees/thing`, holding the same paths.
    let outside = f.plane.join("elsewhere");
    write(&outside, "piece/b/x2.txt", "needle outside\n");
    let mut search = files::search(&[place(&f, "piece")], "needle", literal()).unwrap();
    let stop = AtomicBool::new(false);
    let mut first = Vec::new();
    assert_eq!(
        search.more(1, &stop, &mut |step| first.push(step)),
        Ended::Capped
    );
    assert!(matches!(&first[..], [Searched::File(file)] if file.path == "b/x1.txt"));

    // A chat of the workspace swaps the repo's `.worktrees` folder, an ancestor of the branch.
    let repo_folder = piece.parent().unwrap().to_path_buf();
    std::fs::rename(&repo_folder, f.plane.join("worktrees-was")).unwrap();
    std::os::unix::fs::symlink(&outside, &repo_folder).unwrap();
    let mut rest = Vec::new();
    let ended = search.more(1000, &stop, &mut |step| rest.push(step));

    assert_eq!(ended, Ended::Done);
    for step in &rest {
        if let Searched::File(file) = step {
            assert!(
                file.lines
                    .iter()
                    .all(|line| !line.text().contains("outside")),
                "content from outside the branch was read: {rest:?}"
            );
        }
    }
}
