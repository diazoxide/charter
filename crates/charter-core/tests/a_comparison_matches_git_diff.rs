//! RC-2 (#704): **each kind of comparison matches `git diff` on fixtures** — the file list, each
//! file's mark (A/M/D/R) and rename source, its added and removed line counts, whether it is
//! binary, and each text file's hunks (ADR 0084 §1–§2). `git diff` itself is the oracle, run
//! by the test with no global or system config; the engine under test starts no git.
//!
//! The kinds (R1): (a) a chat's branch against its base, with and without what is not
//! committed; (b) any two refs, from where they meet or exactly; (c) what is not committed,
//! untracked files included and the index untouched; (e) a cross-repo change, a comparison per
//! member in `needs` order.
//!
//! And ADR 0084 §2's fixture: **a repo whose config names a program for every place git runs
//! one** — a diff driver, a text conversion, an external diff, a filter, a pager, the
//! fsmonitor, hooks — on which no comparison and no file's hunks run any of them.

mod support;

use std::path::{Path, PathBuf};

use charter_core::files::{self, Branch, Compared, Comparison, FileDiff, Head, Hunk, Mark};
use charter_core::worktree;

/// The bounded reader, as the app starts it — but this test binary run again, picking
/// [`reader_child`].
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

/// The reader's child: answers the one question it was asked and exits; in any other run it
/// does nothing.
#[test]
fn reader_child() {
    charter_core::unsteered!();
    if let Some(code) = files::serve_if_asked() {
        std::process::exit(code);
    }
}

// ---------------------------------------------------------------------------------------------
// The oracle: git diff, run by the test
// ---------------------------------------------------------------------------------------------

/// git, as the oracle: no global or system config, and `GIT_INDEX_FILE` when given.
fn oracle(dir: &Path, args: &[&str], index: Option<&Path>) -> String {
    let mut cmd = support::unsigned();
    cmd.arg("-C").arg(dir).args(args);
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.env("GIT_CONFIG_NOSYSTEM", "1");
    cmd.env_remove("GIT_DIR");
    if let Some(index) = index {
        cmd.env("GIT_INDEX_FILE", index);
    }
    let out = charter_core::forklock::output(&mut cmd).expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8")
}

/// One file as git lists it.
#[derive(Debug, PartialEq, Eq, Clone)]
struct Listed {
    path: String,
    mark: Mark,
    from: Option<String>,
    /// `None` for binary.
    lines: Option<(u64, u64)>,
}

/// What `git diff` lists for `range` — `["<base>", "<head>"]`, or `["--cached", "<base>"]`
/// against an index holding the working tree — with renames found, as name-status and numstat.
fn git_lists(dir: &Path, range: &[&str], index: Option<&Path>) -> Vec<Listed> {
    let run = |what: &str| {
        let mut args = vec!["diff", "-M", "--no-ext-diff", "--no-textconv", what, "-z"];
        args.extend_from_slice(range);
        oracle(dir, &args, index)
    };
    let names = run("--name-status");
    let mut words = names.split('\0').filter(|w| !w.is_empty());
    let mut out = Vec::new();
    while let Some(status) = words.next() {
        let (mark, from) = match status.chars().next().unwrap() {
            'A' => (Mark::Added, None),
            'D' => (Mark::Deleted, None),
            'M' | 'T' => (Mark::Changed, None),
            'R' => (Mark::Renamed, Some(words.next().unwrap().to_string())),
            other => panic!("unexpected status {other}"),
        };
        let path = words.next().unwrap().to_string();
        out.push(Listed {
            path,
            mark,
            from,
            lines: None,
        });
    }
    let counts = run("--numstat");
    let mut words = counts.split('\0');
    while let Some(word) = words.next() {
        if word.is_empty() {
            continue;
        }
        let mut parts = word.splitn(3, '\t');
        let (added, removed, path) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        let path = if path.is_empty() {
            words.next().unwrap(); // the rename's source
            words.next().unwrap().to_string()
        } else {
            path.to_string()
        };
        let lines = (added != "-").then(|| (added.parse().unwrap(), removed.parse().unwrap()));
        out.iter_mut().find(|one| one.path == path).unwrap().lines = lines;
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// git's `-U0` hunks of one file for `range`.
fn git_hunks(dir: &Path, range: &[&str], paths: &[&str], index: Option<&Path>) -> Vec<Hunk> {
    let mut args = vec![
        "diff",
        "-M",
        "-U0",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
    ];
    args.extend_from_slice(range);
    args.push("--");
    args.extend_from_slice(paths);
    let out = oracle(dir, &args, index);
    out.lines()
        .filter_map(|line| line.strip_prefix("@@ -"))
        .map(|rest| {
            let numbers: Vec<&str> = rest.split(" @@").next().unwrap().split(' ').collect();
            let side = |s: &str| -> (u32, u32) {
                let s = s.trim_start_matches(['-', '+']);
                match s.split_once(',') {
                    Some((start, lines)) => (start.parse().unwrap(), lines.parse().unwrap()),
                    None => (s.parse().unwrap(), 1),
                }
            };
            let (old_start, old_lines) = side(numbers[0]);
            let (new_start, new_lines) = side(numbers[1]);
            Hunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
            }
        })
        .collect()
}

/// An index of its own holding the working tree as it is, for the oracle: `git add -A` into a
/// copy, never into the branch's own index.
fn working_tree_index(dir: &Path, scratch: &Path) -> PathBuf {
    let index = scratch.join(format!("index-{}", rand_name()));
    oracle(dir, &["read-tree", "HEAD"], Some(&index));
    oracle(dir, &["add", "-A"], Some(&index));
    index
}

fn rand_name() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    N.fetch_add(1, Ordering::Relaxed).to_string()
}

fn rev(dir: &Path, spec: &str) -> String {
    oracle(dir, &["rev-parse", spec], None).trim().to_string()
}

// ---------------------------------------------------------------------------------------------
// The engine's answer, in the oracle's shape
// ---------------------------------------------------------------------------------------------

fn listed(compared: &Compared) -> Vec<Listed> {
    compared
        .files
        .iter()
        .map(|one| Listed {
            path: one.path.clone(),
            mark: one.mark,
            from: one.from.clone(),
            lines: one.lines.map(|l| (l.added, l.removed)),
        })
        .collect()
}

/// Every file of `compared` has git's hunks for `range`.
fn every_files_hunks_match(
    f: &support::Fixture,
    at: Branch<'_>,
    dir: &Path,
    compared: &Compared,
    range: &[&str],
    index: Option<&Path>,
) {
    assert!(!compared.files.is_empty(), "a fixture with nothing to show");
    for one in &compared.files {
        let diff = files::compare_file(
            &reader(),
            &f.plane,
            at,
            &compared.sides,
            &one.path,
            one.from.as_deref(),
        )
        .unwrap();
        if one.binary {
            assert_eq!(diff, FileDiff::Binary, "{}", one.path);
            continue;
        }
        let FileDiff::Text { hunks, .. } = diff else {
            panic!("{} is not text: {diff:?}", one.path);
        };
        let mut paths = vec![one.path.as_str()];
        paths.extend(one.from.as_deref());
        assert_eq!(
            hunks,
            git_hunks(dir, range, &paths, index),
            "the hunks of {}",
            one.path
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

fn write(at: &Path, path: &str, text: impl AsRef<[u8]>) {
    let file = at.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn commit_all(at: &Path, message: &str) {
    support::git(at, &["add", "-A"]);
    support::git(at, &["commit", "-q", "-m", message]);
}

/// A body of code with blocks alike enough that where a hunk sits is a choice git's
/// heuristic makes.
fn code(blocks: &[&str]) -> String {
    let mut out = String::new();
    for name in blocks {
        out.push_str(&format!(
            "fn {name}() {{\n    let x = 1;\n    let y = 2;\n    x + y\n}}\n\n"
        ));
    }
    out
}

fn long_text(seed: &str) -> String {
    (0..40)
        .map(|n| format!("{seed} line {n} with enough words to be similar\n"))
        .collect()
}

/// A clone whose `main` holds files of every kind a change can touch, and a piece cut from it.
fn branch_with_files(f: &support::Fixture) -> PathBuf {
    write(&f.clone, "src/lib.rs", code(&["one", "two", "three"]));
    write(&f.clone, "src/gone.rs", "gone\n");
    write(&f.clone, "src/old.rs", long_text("moved"));
    write(&f.clone, "doc/no-eol.txt", "a\nb");
    write(
        &f.clone,
        "assets/logo.bin",
        b"\x89PNG\x00\x01\x02binary\x00",
    );
    write(&f.clone, "keep.txt", "kept\n");
    write(&f.clone, "doc/slider.txt", "1\n2\na\n\nb\n3\n4\n");
    commit_all(&f.clone, "files");
    worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect("a piece is cut")
        .path
}

/// The branch's own commits: a change, an addition, a deletion, a rename with an edit, a last
/// line gaining its line end, a binary changed.
fn commit_on_the_branch(piece: &Path) {
    write(
        piece,
        "src/lib.rs",
        code(&["one", "two", "inserted", "three"]),
    );
    write(piece, "src/new.rs", "new\nfile\n");
    std::fs::remove_file(piece.join("src/gone.rs")).unwrap();
    std::fs::remove_file(piece.join("src/old.rs")).unwrap();
    let mut moved = long_text("moved");
    moved.push_str("one more line\n");
    write(piece, "lib/moved.rs", moved);
    write(piece, "doc/no-eol.txt", "a\nb\nc\n");
    // A hunk git's indent heuristic places (`diff.indentHeuristic`, on by default).
    write(piece, "doc/slider.txt", "1\n2\na\n\nb\na\n\nb\n3\n4\n");
    write(
        piece,
        "assets/logo.bin",
        b"\x89PNG\x00\x01\x02binary-changed\x00",
    );
    commit_all(piece, "the branch's work");
}

/// Work not committed: an edit, a staged rename, a file moved in the working tree only, an
/// untracked file, a deleted file, an ignored one.
fn leave_uncommitted(piece: &Path) {
    write(piece, "keep.txt", "kept\nand edited\n");
    support::git(piece, &["mv", "src/new.rs", "src/renamed.rs"]);
    write(piece, ".gitignore", "*.log\n");
    write(piece, "build.log", "ignored\n");
    write(piece, "notes/untracked.md", "# not tracked yet\n");
    std::fs::remove_file(piece.join("README.md")).unwrap();
    std::fs::rename(piece.join("lib/moved.rs"), piece.join("lib/moved-again.rs")).unwrap();
}

fn piece_of(f: &support::Fixture) -> Branch<'_> {
    Branch::piece(&f.ws, &f.repo, "piece")
}

// ---------------------------------------------------------------------------------------------
// (a) a chat's branch
// ---------------------------------------------------------------------------------------------

#[test]
fn a_branch_against_its_base_matches_git_diff_from_where_it_left_the_base() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    commit_on_the_branch(&piece);
    // The base moves on: what it gained is not the branch's change.
    write(&f.clone, "theirs.txt", "landed on main since\n");
    commit_all(&f.clone, "theirs");
    leave_uncommitted(&piece);

    let compared = files::compare(&reader(), &f.plane, piece_of(&f), &Comparison::Branch).unwrap();

    let fork = oracle(&piece, &["merge-base", "main", "HEAD"], None)
        .trim()
        .to_string();
    let range = [fork.as_str(), "HEAD"];
    assert_eq!(listed(&compared), git_lists(&piece, &range, None));
    assert_eq!(compared.base.as_deref(), Some("main"));
    assert_eq!(compared.sides.base.as_deref(), Some(fork.as_str()));
    assert_eq!(compared.sides.head, Head::Commit(rev(&piece, "HEAD")));
    every_files_hunks_match(&f, piece_of(&f), &piece, &compared, &range, None);
}

#[test]
fn a_branch_with_what_is_not_committed_matches_git_diff_against_the_working_tree() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    commit_on_the_branch(&piece);
    leave_uncommitted(&piece);
    let scratch = tempfile::tempdir().unwrap();

    let compared = files::compare(
        &reader(),
        &f.plane,
        piece_of(&f),
        &Comparison::BranchAndUncommitted,
    )
    .unwrap();

    let fork = oracle(&piece, &["merge-base", "main", "HEAD"], None)
        .trim()
        .to_string();
    let index = working_tree_index(&piece, scratch.path());
    let range = ["--cached", fork.as_str()];
    assert_eq!(listed(&compared), git_lists(&piece, &range, Some(&index)));
    assert_eq!(compared.sides.head, Head::WorkingTree);
    every_files_hunks_match(&f, piece_of(&f), &piece, &compared, &range, Some(&index));
    // The branch's own index was never touched.
    assert!(
        !oracle(&piece, &["diff", "--cached", "--name-only"], None).contains("notes/"),
        "an untracked file was staged"
    );
}

#[test]
fn a_branch_with_no_base_recorded_or_known_is_refused_saying_so() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");

    let said = files::compare(
        &reader(),
        &f.plane,
        Branch::repo(&f.ws, &f.repo),
        &Comparison::Branch,
    )
    .unwrap_err()
    .to_string();

    assert!(said.contains("which branch"), "{said}");
}

/// A branch the repo holds by name, cut from `from`, with one commit of its own: `feature.txt`.
fn feature_from(f: &support::Fixture, from: &str) {
    support::git(&f.clone, &["checkout", "-q", "-b", "feature", from]);
    write(&f.clone, "feature.txt", "the feature\n");
    commit_all(&f.clone, "feature");
    support::git(&f.clone, &["checkout", "-q", "main"]);
}

fn compare_feature(f: &support::Fixture) -> Result<Compared, files::Refused> {
    files::compare(
        &reader(),
        &f.plane,
        Branch::repo(&f.ws, &f.repo),
        &Comparison::NamedBranch {
            branch: "feature".into(),
        },
    )
}

fn inventory_default_branch(f: &support::Fixture, branch: &str) {
    write(
        &f.plane,
        "inventory/repos.json",
        format!(
            r#"{{"repos": [{{"name": "{}", "default_branch": "{branch}"}}]}}"#,
            f.repo
        ),
    );
}

#[test]
fn a_branch_with_no_base_recorded_is_compared_with_the_default_branch_the_inventory_names() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    support::git(&f.clone, &["branch", "trunk"]);
    feature_from(&f, "trunk");
    // Both move on after the cut: neither's commits are the feature's.
    support::git(&f.clone, &["checkout", "-q", "trunk"]);
    write(&f.clone, "trunk.txt", "on trunk since\n");
    commit_all(&f.clone, "trunk");
    support::git(&f.clone, &["checkout", "-q", "main"]);
    write(&f.clone, "main.txt", "on main\n");
    commit_all(&f.clone, "main");
    inventory_default_branch(&f, "trunk");

    let compared = compare_feature(&f).unwrap();

    assert_eq!(compared.base.as_deref(), Some("trunk"));
    assert_eq!(
        listed(&compared),
        git_lists(&f.clone, &["trunk...feature"], None)
    );
}

#[test]
fn a_branch_with_no_base_recorded_or_listed_is_compared_with_what_origin_head_names() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let at = rev(&f.clone, "main");
    support::git(&f.clone, &["update-ref", "refs/remotes/origin/main", &at]);
    support::git(
        &f.clone,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    feature_from(&f, "main");
    write(&f.clone, "main.txt", "local main moved on\n");
    commit_all(&f.clone, "main");

    let compared = compare_feature(&f).unwrap();

    assert_eq!(compared.base.as_deref(), Some("origin/main"));
    assert_eq!(
        listed(&compared),
        git_lists(&f.clone, &["origin/main...feature"], None)
    );
}

#[test]
fn a_default_branch_that_shares_no_history_with_the_branch_is_passed_over_for_the_next() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let at = rev(&f.clone, "main");
    // The inventory names `trunk`; the local `trunk` is unrelated history, and the remote's
    // is where the feature was cut.
    support::git(&f.clone, &["update-ref", "refs/remotes/origin/trunk", &at]);
    support::git(&f.clone, &["checkout", "-q", "--orphan", "trunk"]);
    support::git(&f.clone, &["rm", "-rq", "--cached", "."]);
    write(&f.clone, "unrelated.txt", "another history\n");
    support::git(&f.clone, &["add", "unrelated.txt"]);
    support::git(&f.clone, &["commit", "-q", "-m", "unrelated"]);
    support::git(&f.clone, &["checkout", "-q", "-f", "main"]);
    feature_from(&f, "main");
    inventory_default_branch(&f, "trunk");

    let compared = compare_feature(&f).unwrap();

    assert_eq!(compared.base.as_deref(), Some("origin/trunk"));
    assert_eq!(
        listed(&compared),
        git_lists(&f.clone, &["origin/trunk...feature"], None)
    );
}

// ---------------------------------------------------------------------------------------------
// (b) any two refs
// ---------------------------------------------------------------------------------------------

#[test]
fn two_refs_match_git_diff_from_where_they_meet_and_exactly() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    commit_on_the_branch(&piece);
    write(&f.clone, "theirs.txt", "landed on main since\n");
    write(
        &f.clone,
        "src/lib.rs",
        code(&["zero", "one", "two", "three"]),
    );
    commit_all(&f.clone, "theirs");
    let clone = Branch::repo(&f.ws, &f.repo);
    let tip = rev(&f.clone, "piece");

    for exact in [false, true] {
        let compared = files::compare(
            &reader(),
            &f.plane,
            clone,
            &Comparison::Refs {
                from: "main".into(),
                to: "piece".into(),
                exact,
            },
        )
        .unwrap();

        let range = if exact {
            ["main".to_string(), "piece".to_string()]
        } else {
            ["main...piece".to_string(), String::new()]
        };
        let range: Vec<&str> = range
            .iter()
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(
            listed(&compared),
            git_lists(&f.clone, &range, None),
            "exact: {exact}"
        );
        assert_eq!(compared.sides.head, Head::Commit(tip.clone()));
        every_files_hunks_match(&f, clone, &f.clone, &compared, &range, None);
    }
}

#[test]
fn a_ref_that_names_no_commit_is_refused() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let said = files::compare(
        &reader(),
        &f.plane,
        Branch::repo(&f.ws, &f.repo),
        &Comparison::Refs {
            from: "main".into(),
            to: "no-such-branch".into(),
            exact: false,
        },
    )
    .unwrap_err()
    .to_string();
    assert!(said.contains("no-such-branch"), "{said}");
}

// ---------------------------------------------------------------------------------------------
// (c) what is not committed
// ---------------------------------------------------------------------------------------------

#[test]
fn what_is_not_committed_matches_git_diff_of_head_against_the_working_tree() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    commit_on_the_branch(&piece);
    leave_uncommitted(&piece);
    let scratch = tempfile::tempdir().unwrap();
    let staged_before = oracle(&piece, &["diff", "--cached", "--name-status"], None);
    let index_path = piece.join(oracle(&piece, &["rev-parse", "--git-path", "index"], None).trim());
    let index_before = (
        std::fs::read(&index_path).unwrap(),
        std::fs::metadata(&index_path).unwrap().modified().unwrap(),
    );
    let objects_before = objects(&f.clone);

    let compared =
        files::compare(&reader(), &f.plane, piece_of(&f), &Comparison::Uncommitted).unwrap();
    assert_eq!(
        (
            std::fs::read(&index_path).unwrap(),
            std::fs::metadata(&index_path).unwrap().modified().unwrap(),
        ),
        index_before,
        "the branch's index was rewritten"
    );
    // Read before the oracle's `git add` writes objects of its own.
    assert_eq!(
        objects(&f.clone),
        objects_before,
        "the working tree's files were written to memory, never to the repo"
    );

    let index = working_tree_index(&piece, scratch.path());
    let range = ["--cached", "HEAD"];
    assert_eq!(listed(&compared), git_lists(&piece, &range, Some(&index)));
    assert!(compared.files.iter().all(|one| one.uncommitted));
    assert!(
        compared
            .files
            .iter()
            .any(|one| one.path == "notes/untracked.md" && one.mark == Mark::Added),
        "an untracked file is an added one"
    );
    assert!(!compared.files.iter().any(|one| one.path == "build.log"));
    assert_eq!(compared.sides.base, Some(rev(&piece, "HEAD")));
    every_files_hunks_match(&f, piece_of(&f), &piece, &compared, &range, Some(&index));
    assert_eq!(
        oracle(&piece, &["diff", "--cached", "--name-status"], None),
        staged_before,
        "the index is never touched"
    );
}

/// Every file under the clone's `.git/objects`.
fn objects(clone: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![clone.join(".git/objects")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------------------------
// (e) a cross-repo change
// ---------------------------------------------------------------------------------------------

#[test]
fn a_cross_repo_change_is_one_comparison_per_member_in_needs_order_each_matching_git_diff() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("api");
    let app = f.workspace().join("app");
    std::fs::create_dir_all(&app).unwrap();
    support::git(&app, &["init", "-q", "-b", "main", "."]);
    write(&app, "README.md", "the app\n");
    commit_all(&app, "one");
    for (dir, file) in [(&f.clone, "api.txt"), (&app, "app.txt")] {
        support::git(dir, &["checkout", "-q", "-b", "change/v2"]);
        write(dir, file, "version 2\n");
        commit_all(dir, "v2");
        support::git(dir, &["checkout", "-q", "main"]);
        support::git(dir, &["config", "branch.change/v2.charterBase", "main"]);
    }
    let record = charter_core::change::Record::parse(
        r#"{"change": "v2", "why": "v2 of the api", "created": "2026-10-04T00:00:00+00:00",
            "by": "tests", "members": [
              {"repo": "app", "branch": "change/v2", "needs": ["api"]},
              {"repo": "api", "branch": "change/v2", "needs": []}
            ], "excluded": []}"#,
        "v2",
    )
    .unwrap();
    charter_core::change::store::write(&f.plane, &f.ws, &record).unwrap();

    let members = files::compare_change(&reader(), &f.plane, &f.ws, "v2").unwrap();

    let order: Vec<&str> = members.iter().map(|m| m.repo.as_str()).collect();
    assert_eq!(order, ["api", "app"]);
    for (member, dir) in members.iter().zip([&f.clone, &app]) {
        let compared = member.compared.as_ref().unwrap();
        assert_eq!(
            listed(compared),
            git_lists(dir, &["main...change/v2"], None),
            "{}",
            member.repo
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Nothing the repo's config names runs (ADR 0084 §2)
// ---------------------------------------------------------------------------------------------

#[test]
fn no_program_a_reviewed_repos_config_names_runs_for_any_comparison_or_file() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    commit_on_the_branch(&piece);
    leave_uncommitted(&piece);
    let ran = tempfile::tempdir().unwrap();
    let program = |place: &str| -> String {
        let script = ran.path().join(format!("{place}.sh"));
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ntouch '{}'\ncat\n",
                ran.path().join(format!("ran-{place}")).display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script.display().to_string()
    };
    for (key, place) in [
        ("diff.external", "external"),
        ("diff.x.command", "driver"),
        ("diff.x.textconv", "textconv"),
        ("filter.x.clean", "clean"),
        ("filter.x.smudge", "smudge"),
        ("filter.x.process", "process"),
        ("core.fsmonitor", "fsmonitor"),
        ("core.pager", "pager"),
        ("core.hooksPath", "hooks"),
        ("core.askPass", "askpass"),
        ("core.sshCommand", "ssh"),
        ("credential.helper", "credential"),
        ("gpg.program", "gpg"),
    ] {
        let value = if place == "credential" {
            format!("!{}", program(place))
        } else {
            program(place)
        };
        support::git(&f.clone, &["config", key, &value]);
    }
    write(&piece, ".gitattributes", "* diff=x filter=x\n");

    let at = piece_of(&f);
    let mut all = Vec::new();
    for comparison in [
        Comparison::Branch,
        Comparison::BranchAndUncommitted,
        Comparison::Uncommitted,
        Comparison::Refs {
            from: "main".into(),
            to: "piece".into(),
            exact: false,
        },
    ] {
        let compared = files::compare(&reader(), &f.plane, at, &comparison).unwrap();
        for one in &compared.files {
            files::compare_file(
                &reader(),
                &f.plane,
                at,
                &compared.sides,
                &one.path,
                one.from.as_deref(),
            )
            .unwrap();
        }
        all.push(compared);
    }
    let _ = files::status(&reader(), &f.plane, at).unwrap();

    let ran_any: Vec<String> = std::fs::read_dir(ran.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("ran-"))
        .collect();
    assert!(ran_any.is_empty(), "programs ran: {ran_any:?}");
    assert!(all.iter().all(|c| !c.files.is_empty()));

    // And the fixture is hostile for real: git itself, asked for a plain diff there, runs the
    // diff driver the repo's attributes name (which outranks its external diff).
    let mut cmd = support::unsigned();
    cmd.arg("-C").arg(&piece).args(["diff", "HEAD~1", "HEAD"]);
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_PAGER", "cat");
    let _ = charter_core::forklock::output(&mut cmd);
    assert!(
        ran.path().join("ran-driver").exists() || ran.path().join("ran-external").exists(),
        "git ran none of the fixture's programs: the fixture proves nothing"
    );
}

#[test]
fn a_folder_swapped_for_a_link_is_never_read_through_for_the_working_tree() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = branch_with_files(&f);
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "lib.rs", "SECRET OUTSIDE THE BRANCH\n");
    std::fs::remove_dir_all(piece.join("src")).unwrap();
    std::os::unix::fs::symlink(outside.path(), piece.join("src")).unwrap();

    let compared =
        files::compare(&reader(), &f.plane, piece_of(&f), &Comparison::Uncommitted).unwrap();
    let diff = files::compare_file(
        &reader(),
        &f.plane,
        piece_of(&f),
        &compared.sides,
        "src/lib.rs",
        None,
    );

    // Refused as a file that cannot be read: never read through the link, and never drawn as
    // an empty file either.
    let refused = diff.expect_err("a side read through a swapped folder must be refused");
    assert!(!refused.to_string().contains("SECRET"), "{refused}");
}

#[test]
fn a_file_git_ignores_is_never_read_for_a_files_hunks() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .unwrap()
        .path;
    write(&piece, ".gitignore", ".env\n");
    write(&piece, ".env", "API_TOKEN=sk-live-0123456789abcdef\n");

    let compared =
        files::compare(&reader(), &f.plane, piece_of(&f), &Comparison::Uncommitted).unwrap();
    assert!(!compared.files.iter().any(|one| one.path == ".env"));
    let said = files::compare_file(
        &reader(),
        &f.plane,
        piece_of(&f),
        &compared.sides,
        ".env",
        None,
    )
    .unwrap_err()
    .to_string();

    assert!(!said.contains("sk-live"), "{said}");
    assert!(said.contains("not one of the branch's files"), "{said}");
}

/// Review of RC-2: the working tree's changed files are read for a comparison and the
/// explorer's markers, and many large ones must not swell the reader past its cap, or every
/// marker is lost. Past the comparison's read budget a file is marked and not read.
#[test]
fn many_large_untracked_files_are_still_marked_within_the_readers_memory() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .unwrap()
        .path;
    const LARGE: usize = 15 * 1024 * 1024;
    const MANY: usize = 30;
    // Bytes nothing compresses (the system may compress idle memory): 450 MB of files, each
    // its own content, past the 256 MiB the reader is given here.
    let mut noise = vec![0u8; LARGE];
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for byte in noise.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *byte = x as u8;
    }
    for n in 0..MANY {
        noise[..8].copy_from_slice(&(n as u64).to_le_bytes());
        std::fs::write(piece.join(format!("data-{n:02}.bin")), &noise).unwrap();
    }
    drop(noise);
    write(&piece, "small.txt", "small\n");
    let reader = reader().memory(256 * 1024 * 1024);

    let status = files::status(&reader, &f.plane, piece_of(&f)).unwrap();
    assert_eq!(status.changes.len(), MANY + 1, "{:?}", status.changes);
    assert!(status.changes.iter().all(|one| one.mark == Mark::Added));

    let compared =
        files::compare(&reader, &f.plane, piece_of(&f), &Comparison::Uncommitted).unwrap();
    assert_eq!(compared.files.len(), MANY + 1);
    let small = compared
        .files
        .iter()
        .find(|one| one.path == "small.txt")
        .unwrap();
    assert_eq!(small.lines.map(|l| (l.added, l.removed)), Some((1, 0)));
}

#[test]
fn a_working_tree_file_that_cannot_be_read_is_refused_never_drawn_empty() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .unwrap()
        .path;
    write(&piece, "README.md", "changed\n");
    use std::os::unix::fs::PermissionsExt as _;
    let readme = piece.join("README.md");
    std::fs::set_permissions(&readme, std::fs::Permissions::from_mode(0o000)).unwrap();

    let compared =
        files::compare(&reader(), &f.plane, piece_of(&f), &Comparison::Uncommitted).unwrap();
    let answer = files::compare_file(
        &reader(),
        &f.plane,
        piece_of(&f),
        &compared.sides,
        "README.md",
        None,
    );
    std::fs::set_permissions(&readme, std::fs::Permissions::from_mode(0o644)).unwrap();

    let said = answer.unwrap_err().to_string();
    assert!(said.contains("README.md"), "{said}");
}

#[test]
fn a_file_past_the_preview_size_is_answered_by_its_size() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .unwrap()
        .path;
    let big = "x".repeat(files::LARGEST as usize + 10) + "\n";
    write(&piece, "big.txt", &big);

    let compared =
        files::compare(&reader(), &f.plane, piece_of(&f), &Comparison::Uncommitted).unwrap();
    assert_eq!(compared.files[0].lines.map(|l| l.added), Some(1));
    let diff = files::compare_file(
        &reader(),
        &f.plane,
        piece_of(&f),
        &compared.sides,
        "big.txt",
        None,
    )
    .unwrap();
    assert_eq!(
        diff,
        FileDiff::TooLarge {
            bytes: big.len() as u64
        }
    );
}

#[test]
fn a_path_outside_the_branch_or_into_git_is_refused_for_a_files_hunks() {
    charter_core::unsteered!();
    let f = support::plane_with_clone("thing");
    worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    // The repo's own folder, where `.git` is a folder, and a piece, where it is a file; git's
    // own name in any case, which a case-insensitive folder (macOS, Windows) opens as git's.
    for at in [Branch::repo(&f.ws, &f.repo), piece_of(&f)] {
        let compared = files::compare(&reader(), &f.plane, at, &Comparison::Uncommitted).unwrap();
        for path in [
            "../out.txt",
            ".git/config",
            ".GIT/config",
            ".Git/HEAD",
            ".gIT",
            "sub/.GiT/config",
            "/etc/passwd",
        ] {
            let answer = files::compare_file(&reader(), &f.plane, at, &compared.sides, path, None);
            assert!(answer.is_err(), "{path}: {answer:?}");
            let answer = files::compare_file(
                &reader(),
                &f.plane,
                at,
                &compared.sides,
                "README.md",
                Some(path),
            );
            assert!(answer.is_err(), "from {path}: {answer:?}");
        }
    }
}
