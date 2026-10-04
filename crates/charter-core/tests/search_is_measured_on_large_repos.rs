//! FM-12 (#1115): **⌘P, ⌘⇧F and a branch's status, measured on a large repo** (#1103, V86 F9:
//! no index until a measurement shows the need). One generated repo of many files, shaped like
//! a real one: nested packages, mixed sizes and kinds, some binaries, and a `.gitignore` with
//! build output and dependencies that hold files of their own.
//!
//! What is timed, through the core's public calls the app makes:
//! - **⌘P** (`files::Finder`): a palette session's first find — the listing plus a match, as
//!   the app asks with an empty query when the palette opens — and each keystroke after it;
//! - **⌘⇧F** (`files::search`): a page of [`PAGE_LINES`] lines, as the Search tab asks for one,
//!   timed to its first file of hits and to its end, for common, rare and absent queries; and a
//!   whole scan of the repo, page after page, for a query found nowhere;
//! - **status** (`files::status`, through the bounded reader child): what runs after every
//!   write an agent makes, on a clean branch and on one with a few changes;
//! - **compare** (`files::compare` and `files::compare_file`, the one diff engine, RC-2 #704):
//!   a branch that changed 1,000 files compared with its base — the file list with its line
//!   counts (R3's 1,000-file case), and the first 50 files' hunks as their rows are drawn —
//!   beside `git diff --numstat` timed on the same range; and what is not committed on a clean
//!   branch.
//!
//! It is ignored by default, because building the repo takes minutes. CI runs it nightly and on
//! `main` only, as evidence (V70; `stress.yml`'s `search at scale`), so a regression shows on
//! `main` without ever gating a pull request. CI runs it with `CHARTER_MEASURE_BUDGETS=report`:
//! a shared runner reports each budget it missed and never fails for one (ADR 0086, amended
//! 2026-10-04). The budgets fail the run by hand, on the operator's machine:
//!
//! ```text
//! cargo test --release -p charter-core --test search_is_measured_on_large_repos -- \
//!   --ignored --nocapture --test-threads=1
//! ```
//!
//! - `CHARTER_MEASURE_FILES`: how many tracked files (100,000 by default);
//! - `CHARTER_MEASURE_PLANE`: a folder to build the repo in once and measure again later, so a
//!   run can be timed with the file cache cold (the folder on a volume detached and attached
//!   again in between) and then warm. Unset, the repo is built in a temporary folder;
//! - `CHARTER_MEASURE_ONLY`: one step — `find`, `search:<query name>`, `scan`, `status` or
//!   `compare` — so each cold number is the first read of a cold cache, not one warmed by the
//!   step before;
//! - `CHARTER_MEASURE_BUDGETS=0`: print the numbers without holding them to the budgets;
//! - `CHARTER_MEASURE_BUDGETS=report`: print each budget missed, as a `FM-12 | budget missed`
//!   line, and pass. What `stress.yml` runs.
//!
//! A status also prints gitoxide's own counters for the same compare and `git status`'s time on
//! the same repo ([`probe_status`]), so a slow status says whether it was the disk or a stat
//! that stopped matching the index.
//!
//! **The budgets held** (`docs/spec.md` rows G2–G4). Each is a ceiling well clear of the warm
//! numbers measured on a loaded macOS machine at 300,000 files (#1115). So on that machine a
//! miss is a regression of a different order, not noise. A shared runner is not that machine:
//! GitHub's macOS runners took 8–12 s for a first find the operator's machine does in 0.2–1.7 s,
//! with the repo just written, so CI only reports them:
//! - a ⌘P keystroke's core match, median and worst: within ADR 0086 L1's 50 ms (measured 5.5 and
//!   12.7 ms);
//! - ⌘P's first find: within [`FIRST_FIND`] (measured 0.53–0.57 s);
//! - ⌘⇧F's first file of hits for a common query: within [`FIRST_HIT`] (measured 0.57 s);
//! - a status read: within [`STATUS`], the reader's deadline (measured 15–23 s);
//! - a comparison's file list: within [`STATUS`], the same reader's deadline (measured
//!   0.50–0.51 s for 1,000 changed files with their line counts at 100,000 files, beside
//!   0.20–0.22 s for `git diff --numstat`; a file's hunks 13 ms median, #704).
//!
//! A rare query, or one found nowhere, is measured and held to nothing. Its whole scan takes
//! minutes at this size, which is the index question #1153 carries, not a regression.

mod support;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use charter_core::files::{self, Branch, Ended, Finder, Place, SearchOptions, Searched};

/// The app's page of ⌘⇧F lines (`searchfiles.rs`).
const PAGE_LINES: usize = 200;

/// The app's most ⌘P hits (`findfiles.rs`).
const MOST: usize = 50;

/// ADR 0086 L1, keystroke to screen: the most one keystroke's core match may take.
const KEYSTROKE: Duration = Duration::from_millis(50);

/// ⌘P's first find of a session at the measured size: the listing plus a match.
const FIRST_FIND: Duration = Duration::from_secs(3);

/// ⌘⇧F's first file of hits for a common query.
const FIRST_HIT: Duration = Duration::from_secs(1);

/// One status read through the reader child: the reader's own deadline, past which the read
/// fails and the window says so.
const STATUS: Duration = charter_core::worktree::git::READ;

const WS: &str = "alpha";
const REPO: &str = "big";

/// The bounded reader, as the app starts it, but this test binary run again.
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
    charter_core::unsteered!();
    if let Some(code) = files::serve_if_asked() {
        std::process::exit(code);
    }
}

/// A small, steady random source, so every run builds the same repo.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
        of[self.below(of.len() as u64) as usize]
    }
}

const WORDS: [&str; 24] = [
    "widget", "handler", "service", "model", "view", "util", "config", "parser", "render", "store",
    "client", "server", "index", "types", "hooks", "api", "auth", "cache", "queue", "router",
    "schema", "event", "layout", "session",
];

const AREAS: [&str; 10] = [
    "src",
    "src/core",
    "src/ui",
    "src/net",
    "lib",
    "tests",
    "docs",
    "scripts",
    "src/core/impl",
    "assets",
];

/// The rare word: in about one file in 10,000.
const RARE: &str = "frobnicate_lattice";

/// A line of code-like text.
fn line(dice: &mut Dice, out: &mut String) {
    let a = dice.pick(&WORDS);
    let b = dice.pick(&WORDS);
    let n = dice.below(1000);
    match dice.below(12) {
        0 => out.push_str(&format!("    // TODO: {a} the {b} before {n}\n")),
        1 => out.push_str(&format!("import {{ {a}{n} }} from \"../{b}/{a}\";\n")),
        2 => out.push_str(&format!(
            "fn {a}_{b}_handler(x: u32) -> u32 {{ x + {n} }}\n"
        )),
        3 => out.push_str(&format!("const [{a}, set_{a}] = useState({n});\n")),
        4 => out.push('\n'),
        5 => out.push_str(&format!("    let {a}_{n} = {b}::new();\n")),
        6 => out.push_str(&format!("/// The {a} of a {b}, kept for {n} turns.\n")),
        7 => out.push_str(&format!("    if {a}.len() > {n} {{ return {b}; }}\n")),
        8 => out.push_str(&format!(
            "export function {a}{b}(n) {{ return n * {n}; }}\n"
        )),
        9 => out.push_str(&format!("    {a}.{b}(\"{a}-{n}\")?;\n")),
        10 => out.push_str(&format!("}} // end {a}\n")),
        _ => out.push_str(&format!("    {a} = {b} + {n};\n")),
    }
}

/// How many lines a text file holds: most small, some medium, a few large.
fn lines(dice: &mut Dice) -> u64 {
    match dice.below(100) {
        0..70 => 20 + dice.below(100),
        70..95 => 120 + dice.below(480),
        _ => 600 + dice.below(2400),
    }
}

fn write(at: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, bytes).unwrap();
}

/// Builds the plane with one repo of `count` tracked files at `plane`, and commits them.
fn build(plane: &Path, count: usize) {
    let clone = plane.join("workspaces").join(WS).join(REPO);
    std::fs::create_dir_all(&clone).unwrap();
    support::git(&clone, &["init", "-q", "-b", "main", "."]);
    write(
        &clone.join(".gitignore"),
        b"target/\nnode_modules/\ndist/\n*.log\n.env\n",
    );
    write(&clone.join("README.md"), b"# big\n\nA generated repo.\n");
    let mut dice = Dice(0x05EE_DF12);
    let packages = (count / 2000).max(1);
    let mut text = String::new();
    for n in 0..count.saturating_sub(2) {
        let package = n % packages;
        let area = dice.pick(&AREAS);
        let sub = dice.below(8);
        let name = format!("{}_{}{}", dice.pick(&WORDS), dice.pick(&WORDS), n);
        let kind = dice.below(100);
        let folder = clone.join(format!("packages/pkg{package}/{area}/m{sub}"));
        let (ext, binary) = match kind {
            0..30 => ("rs", false),
            30..55 => ("ts", false),
            55..65 => ("tsx", false),
            65..73 => ("md", false),
            73..80 => ("json", false),
            80..88 => ("py", false),
            88..93 => ("go", false),
            93..97 => ("png", true),
            97..99 => ("wasm", true),
            _ => ("txt", false),
        };
        let at = folder.join(format!("{name}.{ext}"));
        if binary {
            let size = 2048 + dice.below(14 * 1024) as usize;
            let mut bytes = Vec::with_capacity(size);
            bytes.extend_from_slice(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
            while bytes.len() < size {
                bytes.extend_from_slice(&dice.next().to_le_bytes());
            }
            write(&at, &bytes);
            continue;
        }
        text.clear();
        for _ in 0..lines(&mut dice) {
            line(&mut dice, &mut text);
        }
        if dice.below(10_000) == 0 {
            text.push_str(&format!("    {RARE}({n});\n"));
        }
        write(&at, text.as_bytes());
    }
    // Build output and dependencies: ignored, never listed, never read.
    for n in 0..count / 20 {
        write(
            &clone.join(format!("target/debug/deps/m{}/{n}.o", n % 50)),
            b"\0\x01ignored object\n",
        );
        write(
            &clone.join(format!("node_modules/dep{}/lib/{n}.js", n % 200)),
            b"module.exports = 'TODO ignored';\n",
        );
    }
    support::git(&clone, &["add", "-A"]);
    support::git(&clone, &["commit", "-q", "-m", "generated"]);
    std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
}

/// The plane to measure: the one at `CHARTER_MEASURE_PLANE`, built there the first time, or a
/// new one in a temporary folder.
fn plane(count: usize) -> (Option<tempfile::TempDir>, PathBuf) {
    if let Some(at) = std::env::var_os("CHARTER_MEASURE_PLANE") {
        let at = PathBuf::from(at);
        if !at.join("charter.toml").exists() {
            let started = Instant::now();
            build(&at, count);
            println!("FM-12 | built {count} files in {:?}", started.elapsed());
        }
        return (None, std::fs::canonicalize(at).unwrap());
    }
    let dir = tempfile::tempdir().unwrap();
    let at = std::fs::canonicalize(dir.path()).unwrap();
    let started = Instant::now();
    build(&at, count);
    println!("FM-12 | built {count} files in {:?}", started.elapsed());
    (Some(dir), at)
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn median(mut all: Vec<Duration>) -> Duration {
    all.sort();
    all[all.len() / 2]
}

/// The queries ⌘⇧F is timed with: a name, the query, whether it is a regex.
const QUERIES: [(&str, &str, bool); 5] = [
    ("common", "TODO", false),
    ("word", "useState", false),
    ("regex", r"fn \w+_router_handler\(", true),
    ("rare", RARE, false),
    ("absent", "qqzx_found_nowhere", false),
];

/// What one step measured, and whether it kept its budget.
struct Held {
    missed: Vec<String>,
    budgets: bool,
}

/// How the budgets are held, from `CHARTER_MEASURE_BUDGETS`: `0` not at all, `report` reported
/// and never failed (CI's shared runners, ADR 0086), anything else failed (the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Budgets {
    Off,
    Report,
    Fail,
}

impl Budgets {
    fn from(value: Option<&str>) -> Self {
        match value {
            Some("0") => Self::Off,
            Some("report") => Self::Report,
            _ => Self::Fail,
        }
    }
}

/// What a run with `budgets` does with the budgets it `missed`: the lines it prints, and
/// whether it fails.
fn verdict(budgets: Budgets, count: usize, missed: &[String]) -> (Vec<String>, bool) {
    match budgets {
        Budgets::Off => (Vec::new(), false),
        Budgets::Report => (
            missed
                .iter()
                .map(|one| format!("FM-12 | budget missed | {count} files | {one}"))
                .collect(),
            false,
        ),
        Budgets::Fail => (Vec::new(), !missed.is_empty()),
    }
}

#[test]
fn a_reported_budget_is_printed_and_never_fails_the_run() {
    charter_core::unsteered!();
    let missed = vec!["⌘P first find (first): 8082.3 ms past 3000.0 ms".to_string()];
    assert_eq!(
        verdict(Budgets::Report, 100_000, &missed),
        (
            vec![
                "FM-12 | budget missed | 100000 files | ⌘P first find (first): 8082.3 ms past \
                 3000.0 ms"
                    .to_string()
            ],
            false
        )
    );
    assert_eq!(verdict(Budgets::Fail, 100_000, &missed), (Vec::new(), true));
    assert_eq!(verdict(Budgets::Fail, 100_000, &[]), (Vec::new(), false));
    assert_eq!(verdict(Budgets::Off, 100_000, &missed), (Vec::new(), false));
    assert_eq!(Budgets::from(None), Budgets::Fail);
    assert_eq!(Budgets::from(Some("report")), Budgets::Report);
    assert_eq!(Budgets::from(Some("0")), Budgets::Off);
    assert_eq!(Budgets::from(Some("1")), Budgets::Fail);
}

impl Held {
    fn within(&mut self, what: &str, took: Duration, budget: Duration) {
        if self.budgets && took > budget {
            self.missed
                .push(format!("{what}: {} past {}", ms(took), ms(budget)));
        }
    }
}

fn measure_find(scope: &[Place<'_>], held: &mut Held) {
    for session in ["first", "second"] {
        let mut finder = Finder::default();
        let opened = Instant::now();
        let answer = finder.find(scope, "", MOST);
        let first = opened.elapsed();
        assert!(answer.refused.is_empty(), "{:?}", answer.refused);
        let typed = "routerhandler";
        let mut each = Vec::new();
        for end in 1..=typed.len() {
            let one = Instant::now();
            let answer = finder.find(scope, &typed[..end], MOST);
            each.push(one.elapsed());
            assert!(!answer.hits.is_empty(), "{}", &typed[..end]);
        }
        let worst = *each.iter().max().unwrap();
        let mid = median(each.clone());
        println!(
            "FM-12 | find | {session} session: first find {}{}; keystrokes median {}, worst {}",
            ms(first),
            if answer.partial.is_empty() {
                String::new()
            } else {
                format!(" ({})", answer.partial[0].1)
            },
            ms(mid),
            ms(worst)
        );
        held.within(&format!("⌘P first find ({session})"), first, FIRST_FIND);
        held.within(&format!("⌘P keystroke median ({session})"), mid, KEYSTROKE);
        held.within(&format!("⌘P keystroke worst ({session})"), worst, KEYSTROKE);
    }
}

fn measure_search(scope: &[Place<'_>], name: &str, held: &mut Held) {
    let (_, query, regex) = QUERIES
        .iter()
        .find(|(called, ..)| *called == name)
        .expect("a query of QUERIES");
    let options = SearchOptions {
        regex: *regex,
        ..SearchOptions::default()
    };
    let stop = AtomicBool::new(false);
    let mut search = files::search(scope, query, options).expect("a query that searches");
    let started = Instant::now();
    let mut first: Option<Duration> = None;
    let mut files_hit = 0usize;
    let mut lines = 0usize;
    let ended = search.more(PAGE_LINES, &stop, &mut |heard| {
        if let Searched::File(hits) = heard {
            first.get_or_insert_with(|| started.elapsed());
            files_hit += 1;
            lines += hits.lines.len();
        }
    });
    let page = started.elapsed();
    println!(
        "FM-12 | search:{name} | {query:?}: first hit {}, page {} ({ended:?}, {files_hit} files, \
         {lines} lines)",
        first.map_or("none".to_string(), ms),
        ms(page),
    );
    if name == "common" {
        held.within(
            "⌘⇧F first hit (common)",
            first.unwrap_or(Duration::MAX),
            FIRST_HIT,
        );
    }
}

/// A query found nowhere, run page after page to the end of the repo: the whole scan's cost.
fn measure_scan(scope: &[Place<'_>]) {
    let stop = AtomicBool::new(false);
    let mut search = files::search(scope, "qqzx_found_nowhere", SearchOptions::default())
        .expect("a query that searches");
    let started = Instant::now();
    let mut pages = 0;
    loop {
        pages += 1;
        if search.more(PAGE_LINES, &stop, &mut |_| {}) == Ended::Done {
            break;
        }
    }
    println!(
        "FM-12 | scan | the whole repo, nothing found: {} over {pages} page(s) of at most {:?}",
        ms(started.elapsed()),
        files::PAGE_TIME
    );
}

fn measure_status(plane: &Path, clone: &Path, held: &mut Held) {
    let reader = reader();
    let branch = Branch::repo(WS, REPO);
    let mut clean = Vec::new();
    for _ in 0..3 {
        let one = Instant::now();
        let status = match files::status(&reader, plane, branch) {
            Ok(status) => status,
            // The reader's own deadline or memory cap ended the read: on a slow disk at this
            // size that is the measurement, a status read missed by more than its budget, and
            // it is said like any other miss rather than ending the run (ADR 0086).
            Err(why) => {
                held.missed
                    .push(format!("status read: no answer ({why:?})"));
                return;
            }
        };
        clean.push(one.elapsed());
        assert!(status.changes.is_empty(), "{:?}", status.changes);
    }
    // An agent's few writes: five files changed, five added.
    let mut changed = Vec::new();
    for n in 0..5 {
        std::fs::write(clone.join(format!("agent_added_{n}.rs")), "fn added() {}\n").unwrap();
    }
    let readme = clone.join("README.md");
    let before = std::fs::read(&readme).unwrap();
    std::fs::write(&readme, "# big\n\nChanged by an agent.\n").unwrap();
    let mut answered = true;
    for _ in 0..3 {
        let one = Instant::now();
        match files::status(&reader, plane, branch) {
            Ok(status) => {
                changed.push(one.elapsed());
                assert_eq!(status.changes.len(), 6, "{:?}", status.changes);
            }
            Err(why) => {
                held.missed
                    .push(format!("status read: no answer ({why:?})"));
                answered = false;
                break;
            }
        }
    }
    std::fs::write(&readme, before).unwrap();
    for n in 0..5 {
        std::fs::remove_file(clone.join(format!("agent_added_{n}.rs"))).unwrap();
    }
    if !answered {
        return;
    }
    let say = |all: &[Duration]| all.iter().map(|d| ms(*d)).collect::<Vec<_>>().join(", ");
    println!(
        "FM-12 | status | clean: {}; six changes: {}",
        say(&clean),
        say(&changed)
    );
    for one in clean.iter().chain(&changed) {
        held.within("status read", *one, STATUS);
    }
    probe_status(clone);
}

/// What a status costs on this machine apart from charter: gitoxide's own counters for the
/// same compare the reader runs, in this process, and `git status` on the same clean repo.
/// A status far over its measured value with every entry trusted by its stat (no file read,
/// none racily clean, none to update) is the disk's cost, not charter's; one with files read
/// is a stat that stopped matching the index, which the reader, never writing the index,
/// pays again on every read.
fn probe_status(clone: &Path) {
    let repo = gix::open(clone).expect("the clone opens");
    let started = Instant::now();
    let mut items = repo
        .status(gix::progress::Discard)
        .expect("a status")
        .untracked_files(gix::status::UntrackedFiles::Files)
        .index_worktree_submodules(None)
        .into_iter(None)
        .expect("a status");
    for item in items.by_ref() {
        item.expect("an item");
    }
    let took = started.elapsed();
    let outcome = items.into_outcome().expect("an outcome");
    let tracked = &outcome.index_worktree.tracked_file_modification;
    println!(
        "FM-12 | status | gix in process: {}; {} entries, {} stat calls, {} racily clean, {} to \
         update, {} files read ({} bytes)",
        ms(took),
        tracked.entries_processed,
        tracked.symlink_metadata_calls,
        tracked.racy_clean,
        tracked.entries_to_update,
        tracked.worktree_files_read,
        tracked.worktree_bytes,
    );
    let mut git = Vec::new();
    for _ in 0..3 {
        let one = Instant::now();
        let out = charter_core::forklock::output(
            std::process::Command::new("git")
                .args(["--no-optional-locks", "status", "--porcelain", "-uall"])
                .current_dir(clone),
        )
        .expect("git runs");
        assert!(out.status.success(), "{out:?}");
        git.push(one.elapsed());
    }
    println!(
        "FM-12 | status | git status (no index write): {}",
        git.iter().map(|d| ms(*d)).collect::<Vec<_>>().join(", ")
    );
}

/// The branch [`measure_compare`] compares: made once in the repo, 1,000 files changed.
const REVIEWED: &str = "reviewed";

/// How many files the reviewed branch changes: R3's 1,000-file diff.
const REVIEWED_FILES: usize = 1000;

/// Makes [`REVIEWED`] in `clone` unless it is there: 985 text files edited (a line inserted in
/// the middle and one changed), 5 deleted, 5 renamed and 5 added, in one commit.
fn reviewed_branch(clone: &Path) {
    let mut asked = support::unsigned();
    asked
        .arg("-C")
        .arg(clone)
        .args(["rev-parse", "--verify", "--quiet", REVIEWED])
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    let exists = charter_core::forklock::output(&mut asked).is_ok_and(|out| out.status.success());
    if exists {
        return;
    }
    let listed = support::git(clone, &["ls-files", "-z"]);
    let text: Vec<String> = String::from_utf8_lossy(&listed.stdout)
        .split('\0')
        .filter(|p| p.ends_with(".rs") || p.ends_with(".ts") || p.ends_with(".md"))
        .take(REVIEWED_FILES)
        .map(str::to_string)
        .collect();
    support::git(clone, &["checkout", "-q", "-b", REVIEWED]);
    let (edited, rest) = text.split_at(REVIEWED_FILES - 15);
    for path in edited {
        let at = clone.join(path);
        let old = std::fs::read_to_string(&at).unwrap();
        let lines: Vec<&str> = old.lines().collect();
        let middle = lines.len() / 2;
        let mut new = String::new();
        for (n, line) in lines.iter().enumerate() {
            if n == middle {
                new.push_str("    let reviewed = true; // inserted by the review fixture\n");
            }
            if n == 1 {
                new.push_str("// changed by the review fixture\n");
                continue;
            }
            new.push_str(line);
            new.push('\n');
        }
        std::fs::write(&at, new).unwrap();
    }
    for path in &rest[..5] {
        std::fs::remove_file(clone.join(path)).unwrap();
    }
    for path in &rest[5..10] {
        support::git(clone, &["mv", path, &format!("{path}.moved")]);
    }
    for n in 0..5 {
        write(
            &clone.join(format!("reviewed/added_{n}.rs")),
            b"fn added_by_review() {}\n",
        );
    }
    support::git(clone, &["add", "-A"]);
    support::git(clone, &["commit", "-q", "-m", "reviewed"]);
    support::git(clone, &["checkout", "-q", "main"]);
}

fn measure_compare(plane: &Path, clone: &Path, held: &mut Held) {
    let made = Instant::now();
    reviewed_branch(clone);
    println!(
        "RC-2 | compare | the reviewed branch ready in {}",
        ms(made.elapsed())
    );
    let range = format!("main...{REVIEWED}");
    let mut git_took = Vec::new();
    for _ in 0..3 {
        let one = Instant::now();
        let out = support::git(clone, &["diff", "-M", "--numstat", "-z", &range]);
        git_took.push(one.elapsed());
        assert!(!out.stdout.is_empty());
    }
    let reader = reader();
    let branch = Branch::repo(WS, REPO);
    let comparison = files::Comparison::Refs {
        from: "main".into(),
        to: REVIEWED.into(),
        exact: false,
    };
    let mut listed = Vec::new();
    let mut compared = None;
    for _ in 0..3 {
        let one = Instant::now();
        let answer = files::compare(&reader, plane, branch, &comparison).expect("a comparison");
        listed.push(one.elapsed());
        assert_eq!(
            answer.files.len(),
            REVIEWED_FILES,
            "{:?}",
            &answer.files[..3]
        );
        compared = Some(answer);
    }
    let compared = compared.unwrap();
    let mut rows = Vec::new();
    for one in compared.files.iter().take(50) {
        let started = Instant::now();
        files::compare_file(
            &reader,
            plane,
            branch,
            &compared.sides,
            &one.path,
            one.from.as_deref(),
        )
        .expect("a file's hunks");
        rows.push(started.elapsed());
    }
    let mut uncommitted = Vec::new();
    for _ in 0..3 {
        let one = Instant::now();
        let answer = files::compare(&reader, plane, branch, &files::Comparison::Uncommitted)
            .expect("a comparison");
        uncommitted.push(one.elapsed());
        assert!(answer.files.is_empty(), "{:?}", answer.files);
    }
    let say = |all: &[Duration]| all.iter().map(|d| ms(*d)).collect::<Vec<_>>().join(", ");
    println!(
        "RC-2 | compare | {REVIEWED_FILES} files, list with line counts: {}; git diff --numstat: \
         {}",
        say(&listed),
        say(&git_took)
    );
    println!(
        "RC-2 | compare | one file's hunks, 50 rows: median {}, worst {}, total {}",
        ms(median(rows.clone())),
        ms(*rows.iter().max().unwrap()),
        ms(rows.iter().sum())
    );
    println!(
        "RC-2 | compare | nothing uncommitted: {}",
        say(&uncommitted)
    );
    for one in &listed {
        held.within("a comparison's file list", *one, STATUS);
    }
}

#[test]
#[ignore = "builds a repo of 100,000 files or more; run by hand, or by stress.yml nightly"]
fn search_on_a_large_repo_stays_within_its_budgets() {
    charter_core::unsteered!();
    let count: usize = std::env::var("CHARTER_MEASURE_FILES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(100_000);
    let only = std::env::var("CHARTER_MEASURE_ONLY").ok();
    let budgets = Budgets::from(std::env::var("CHARTER_MEASURE_BUDGETS").ok().as_deref());
    let mut held = Held {
        missed: Vec::new(),
        budgets: budgets != Budgets::Off,
    };
    let (_dir, plane) = plane(count);
    let clone = plane.join("workspaces").join(WS).join(REPO);
    let scope = [Place {
        plane: &plane,
        branch: Branch::repo(WS, REPO),
    }];
    let runs = |step: &str| only.as_deref().is_none_or(|one| one == step);
    if runs("find") {
        measure_find(&scope, &mut held);
    }
    for (name, ..) in QUERIES {
        if runs(&format!("search:{name}")) {
            measure_search(&scope, name, &mut held);
        }
    }
    if runs("scan") {
        measure_scan(&scope);
    }
    if runs("status") {
        measure_status(&plane, &clone, &mut held);
    }
    if runs("compare") {
        measure_compare(&plane, &clone, &mut held);
    }
    let (lines, fails) = verdict(budgets, count, &held.missed);
    for line in lines {
        println!("{line}");
    }
    assert!(
        !fails,
        "past the budget at {count} files: {:?}",
        held.missed
    );
}
