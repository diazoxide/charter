//! ⌘⇧F: **the content of a branch's files, searched** (FM-8, #1111; #1103, V86 F9), across a
//! scope of branches: one branch, a workspace's, a project's, or every open project's.
//!
//! **A live scan, in-process, on ripgrep's own crates.** `ignore` walks the branch's folder,
//! never following a link; `grep-regex` and `grep-searcher` match line by line. No index is kept
//! and no program is started per file. The one git call a branch costs is its offered list, the
//! same listing [`super::list`] and ⌘P read: operator-initiated git, which stays on the git
//! binary (V88a), through the hardened runner.
//!
//! **What is read is what the light editor opens, never more.** git's offered list — what it
//! tracks, and what it does not track and does not ignore — is the only rule: the walk reads no
//! ignore file of its own and descends only into folders that hold an offered path. On top of
//! that it never reads:
//! - a link, wherever it leads, or anything in `.git` or a repository nested in the branch;
//! - a path in a vault or charter's own state, by the leak guard's one pattern for it;
//! - a file named like a credential (`.env`, a private key, `*.pem`, `credentials.json`, a
//!   registry's `.npmrc`/`.pypirc`), by the repo save's rule for it, even where git offers it;
//! - a binary file or an image, by the preview's own test of its first bytes;
//! - a file past the preview's [`super::LARGEST`], which the preview would not draw either.
//!
//! A file is opened from the branch's folder one component at a time, following no link, so a
//! folder or the file swapped for a link after the walk saw it is refused, not followed.
//!
//! **Paged, stoppable, resumable.** A [`Search`] walks its scope in order — place by place,
//! each branch's folders by name — and [`Search::more`] runs it until a page of lines has been
//! heard, its time budget is spent, the caller's stop is raised, or nothing is left. Stop and
//! time are heard inside a file too, a [`FILL`] at a time, and a line longer than
//! [`LONGEST_LINE`] is not matched at all. The walk's place is kept between pages, so "Show
//! more" continues where the page stopped rather than starting again.
//!
//! **Read on several threads, heard in walk order** (#1153). A rare query reads every offered
//! file, so one thread reading them one after another left the disk mostly idle. The walk stays
//! on the caller's thread and hands each searchable file to [`READERS`] threads, at most
//! [`AHEAD`] files ahead of the one heard next; what they find is heard strictly in walk order, so
//! a page, and "Show more" after it, says the same files in the same order as one thread would.
//! A file read ahead but not heard when the page ends is read again by the next page, so
//! nothing heard is older than its page.

use std::collections::VecDeque;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use grep_matcher::Matcher as _;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::sinks::Lossy;
use grep_searcher::{BinaryDetection, SearcherBuilder};

use super::{Named, Place, Refused};

/// The longest query searched, in characters: past a sentence, a search is a paste by mistake.
pub const LONGEST_QUERY: usize = 1000;

/// The most a query's compiled regex may take: 1 MiB, a tenth of the regex crate's own default,
/// so a pattern like `\w{1000}{1000}` is refused as too big rather than built.
const REGEX_SIZE: usize = 1 << 20;

/// The most the lazy DFA may hold while matching: 8 MiB, past which it falls back to a slower
/// engine rather than grow.
const DFA_SIZE: usize = 8 << 20;

/// The most matching lines one file shows. The count still says how many there are.
pub const FILE_LINES: usize = 100;

/// The most characters of one matching line kept, around its first match: a minified bundle's
/// single line is shown as the stretch that matched.
pub const LINE_CHARS: usize = 240;

/// How long one page may run before it stops and offers to continue: a scope of a million
/// files is walked a page at a time, never all at once behind one keystroke.
pub const PAGE_TIME: Duration = Duration::from_secs(10);

/// How a query is matched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchOptions {
    /// The query is a regular expression (Rust's `regex` syntax); otherwise its every character
    /// is itself.
    pub regex: bool,
    /// Letters match only in the case typed; otherwise any case.
    pub match_case: bool,
    /// A match must be a whole word: no letter, digit or `_` either side.
    pub whole_word: bool,
}

/// One stretch of a matching line: what it says, and whether it is what matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub text: String,
    pub hit: bool,
}

/// One matching line of a file: its number, from 1, and its text in [`Part`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitLine {
    pub number: u64,
    pub parts: Vec<Part>,
    /// Whether text before or after the parts was left out, for a line past [`LINE_CHARS`].
    pub clipped: bool,
}

impl HitLine {
    /// The line's text as kept, its parts joined.
    pub fn text(&self) -> String {
        self.parts.iter().map(|part| part.text.as_str()).collect()
    }
}

/// One file with matches: which place of the scope, by its index; its path in that branch;
/// how many of its lines match; and the first [`FILE_LINES`] of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHits {
    pub at: usize,
    pub path: String,
    pub count: u64,
    pub lines: Vec<HitLine>,
}

/// What a search hears, in the order the scope is walked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Searched {
    /// A file with matches.
    File(FileHits),
    /// A place of the scope that could not be searched, by its index, in the core's sentence.
    Refused { at: usize, why: String },
    /// A file of a place that was not searched, and why: a line past [`LONGEST_LINE`], or
    /// matching it took past the page's time.
    NotSearched {
        at: usize,
        path: String,
        why: String,
    },
}

/// Why a page of a search ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// Every place of the scope is searched.
    Done,
    /// The page's lines were heard; [`Search::more`] continues.
    Capped,
    /// The page's time ran out; [`Search::more`] continues.
    OutOfTime,
    /// The caller's stop was raised.
    Stopped,
}

/// Why a query is not searched.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BadQuery {
    #[error("Type something to search for")]
    Empty,
    #[error("A search is at most {LONGEST_QUERY} characters")]
    TooLong,
    #[error("A search is one line")]
    Lines,
    #[error("{0}")]
    Pattern(String),
}

/// A content search of a scope, walked a page at a time.
pub struct Search {
    places: Vec<(PathBuf, Named)>,
    matcher: RegexMatcher,
    /// The place being walked, by index, and how far.
    at: usize,
    walking: Option<Walking>,
    budget: Duration,
    /// What finds each branch's folder: the bounded reader's child, or, with none, this process.
    reader: Option<super::Reader>,
}

/// One branch, being walked.
struct Walking {
    base: PathBuf,
    /// The branch's folder, held open from the project's root: every file is opened relative to
    /// it, so no folder above the file is looked up by path again.
    held: super::Held,
    offered: std::sync::Arc<Vec<String>>,
    walk: ignore::Walk,
    /// Files the walk handed out that no page has heard yet, in walk order: read ahead by the
    /// page that ended, and read again by the next.
    unheard: VecDeque<String>,
}

/// How many threads read a page's files at once: the machine's cores, at most four. A rare
/// query's page is bound by the system's file reads, not by matching: on the measuring Mac
/// (#1153) eight readers took the same time as four and three times the system's CPU, and the
/// app shares its machine with the agents' builds.
const READERS: usize = 4;

/// How many files past the next one to be heard are handed to the readers: enough to keep each
/// busy, few enough that a page ending throws away little that was read.
const AHEAD: usize = 4 * READERS;

/// How many readers this machine gets.
fn readers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().min(READERS))
}

/// A search of `scope` for `query`, matched as `options` say. Nothing is read until
/// [`Search::more`] is asked; a query that cannot be searched is refused here.
pub fn search(
    scope: &[Place<'_>],
    query: &str,
    options: SearchOptions,
) -> Result<Search, BadQuery> {
    if query.is_empty() {
        return Err(BadQuery::Empty);
    }
    if query.chars().count() > LONGEST_QUERY {
        return Err(BadQuery::TooLong);
    }
    if query.contains(['\n', '\r']) {
        return Err(BadQuery::Lines);
    }
    let matcher = RegexMatcherBuilder::new()
        .fixed_strings(!options.regex)
        .case_insensitive(!options.match_case)
        .word(options.whole_word)
        .line_terminator(Some(b'\n'))
        .size_limit(REGEX_SIZE)
        .dfa_size_limit(DFA_SIZE)
        .build(query)
        .map_err(|e| BadQuery::Pattern(e.to_string()))?;
    let places = scope
        .iter()
        .map(|place| {
            (
                place.plane.to_path_buf(),
                Named {
                    ws: place.branch.ws.to_string(),
                    repo: place.branch.repo.to_string(),
                    piece: place.branch.piece.map(str::to_owned),
                },
            )
        })
        .collect();
    Ok(Search {
        places,
        matcher,
        at: 0,
        walking: None,
        budget: PAGE_TIME,
        reader: None,
    })
}

impl Search {
    /// The same search with another page time: for a test, or a caller with its own clock.
    pub fn with_page_time(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    /// The same search, finding each branch's folder by `reader`'s child, so walking a branch
    /// starts no git to find its folder in this process (#1189).
    pub fn reading_with(mut self, reader: super::Reader) -> Self {
        self.reader = Some(reader);
        self
    }

    /// Searches on from where the last page stopped, telling `heard` each file with matches,
    /// each file that could not be searched and each place that could not be searched, until
    /// `lines` matching lines have been heard (counting whole files, so a page may run a file
    /// past it), the page's time is spent, `stop` is raised, or the scope is done.
    ///
    /// **Stop and the page's time are heard inside a file too**: its bytes reach the matcher a
    /// [`FILL`] at a time, and each fill asks both first. So a costly pattern over a large file
    /// ends within one fill's matching, never at the end of the file.
    pub fn more(
        &mut self,
        lines: usize,
        stop: &AtomicBool,
        heard: &mut dyn FnMut(Searched),
    ) -> Ended {
        let until = Instant::now() + self.budget;
        let mut shown = 0usize;
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ended::Stopped;
            }
            if shown > 0 && shown >= lines {
                return Ended::Capped;
            }
            if Instant::now() >= until {
                return Ended::OutOfTime;
            }
            if self.at >= self.places.len() {
                return Ended::Done;
            }
            if self.walking.is_none() {
                let (plane, named) = &self.places[self.at];
                match walking(self.reader.as_ref(), plane, named, stop) {
                    Ok(Some(walk)) => self.walking = Some(walk),
                    // Called off while git listed the branch: the place is listed again by the
                    // next page, so a stop costs nothing it had found.
                    Ok(None) => return Ended::Stopped,
                    Err(why) => {
                        heard(Searched::Refused { at: self.at, why });
                        self.at += 1;
                        continue;
                    }
                }
            }
            let walking = self.walking.as_mut().expect("set above");
            let page = Page {
                matcher: &self.matcher,
                at: self.at,
                lines,
                stop,
                until,
            };
            match page.run(walking, &mut shown, heard) {
                Some(ended) => return ended,
                None => {
                    self.walking = None;
                    self.at += 1;
                }
            }
        }
    }
}

/// One page's run over one branch.
struct Page<'a> {
    matcher: &'a RegexMatcher,
    at: usize,
    lines: usize,
    stop: &'a AtomicBool,
    until: Instant,
}

/// A file handed to the readers, and what they found once they have.
struct Slot {
    path: String,
    read: Option<Read>,
}

impl Page<'_> {
    /// Searches `walking` on, the readers reading ahead and each file heard in walk order, until
    /// the page ends — `Some` of why — or the branch is done: `None`. `shown` counts the page's
    /// lines heard so far, across the places it has walked.
    fn run(
        &self,
        walking: &mut Walking,
        shown: &mut usize,
        heard: &mut dyn FnMut(Searched),
    ) -> Option<Ended> {
        let Walking {
            base,
            held,
            offered,
            walk,
            unheard,
        } = walking;
        let held: &super::Held = held;
        // Raised once the page has ended: what a reader is still matching is cut, as by a stop.
        let over = AtomicBool::new(false);
        let watch = Watch {
            stop: self.stop,
            over: &over,
            until: self.until,
        };
        let mut slots: VecDeque<Slot> = VecDeque::new();
        let (ask, asked) = mpsc::channel::<(usize, String)>();
        let asked = Mutex::new(asked);
        let (tell, told) = mpsc::channel::<(usize, Read)>();
        let (ended, panicked) = std::thread::scope(|scope| {
            let ask = ask;
            let mut readers_of_page = Vec::new();
            for _ in 0..readers() {
                let (asked, tell, watch) = (&asked, tell.clone(), &watch);
                readers_of_page.push(scope.spawn(move || {
                    loop {
                        let next = asked.lock().map(|asked| asked.recv());
                        let Ok(Ok((seq, path))) = next else { return };
                        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            file_hits(self.matcher, self.at, held, path, watch)
                        }));
                        match read {
                            Ok(read) => {
                                if tell.send((seq, read)).is_err() {
                                    return;
                                }
                            }
                            // A reader that panicked still answers its file, as cut, and ends
                            // the page, so nobody waits on it; its panic is then raised on the
                            // caller's thread.
                            Err(panic) => {
                                let _ = tell.send((seq, Read::Cut(Cut::Stop)));
                                watch.over.store(true, Ordering::Relaxed);
                                std::panic::resume_unwind(panic);
                            }
                        }
                    }
                }));
            }
            drop(tell);
            // `slots[0]` is the file heard next, numbered `first`; `first + slots.len()` is the
            // next file handed out.
            let mut first = 0usize;
            let mut walked = false;
            let ended = 'page: loop {
                while slots.len() < AHEAD + 1 {
                    let path = match unheard.pop_front() {
                        Some(path) => path,
                        None if walked => break,
                        None => match next_searchable(walk, base, offered) {
                            Some(path) => path,
                            None => {
                                walked = true;
                                break;
                            }
                        },
                    };
                    if ask.send((first + slots.len(), path.clone())).is_err() {
                        break;
                    }
                    slots.push_back(Slot { path, read: None });
                }
                if slots.is_empty() {
                    break 'page None;
                }
                while slots.front().is_some_and(|slot| slot.read.is_some()) {
                    let Slot { path, read } = slots.pop_front().expect("one is there");
                    first += 1;
                    match read.expect("one is there") {
                        Read::Hits(found) => {
                            *shown += found.lines.len();
                            heard(Searched::File(found));
                        }
                        Read::Nothing => {}
                        Read::NotSearched(path, why) => heard(Searched::NotSearched {
                            at: self.at,
                            path,
                            why,
                        }),
                        Read::Cut(Cut::Stop) => {
                            slots.push_front(Slot { path, read: None });
                            break 'page Some(Ended::Stopped);
                        }
                        // The file heard next was cut by the page's end, as one thread would have
                        // been: it is said, and the page goes on past it. Files behind it that
                        // were cut too are read again by the next page.
                        Read::Cut(Cut::Time(path)) => {
                            heard(Searched::NotSearched {
                                at: self.at,
                                path,
                                why: "matching it took longer than a page may".to_string(),
                            });
                            break 'page Some(Ended::OutOfTime);
                        }
                    }
                    if self.stop.load(Ordering::Relaxed) {
                        break 'page Some(Ended::Stopped);
                    }
                    if *shown > 0 && *shown >= self.lines {
                        break 'page Some(Ended::Capped);
                    }
                    if Instant::now() >= self.until {
                        break 'page Some(Ended::OutOfTime);
                    }
                }
                if slots.is_empty() {
                    continue;
                }
                // Every reader cuts its file at the stop or the page's end, so this wait ends.
                match told.recv() {
                    Ok((seq, read)) => {
                        if let Some(slot) = seq.checked_sub(first).and_then(|at| slots.get_mut(at))
                        {
                            slot.read = Some(read);
                        }
                    }
                    Err(_) => break 'page Some(Ended::OutOfTime),
                }
            };
            over.store(true, Ordering::Relaxed);
            drop(ask);
            // Every reader joined here, so the first one's panic reaches the caller as itself.
            let panicked = readers_of_page.into_iter().fold(None, |first, reader| {
                let panic = reader.join().err();
                first.or(panic)
            });
            (ended, panicked)
        });
        if let Some(panic) = panicked {
            std::panic::resume_unwind(panic);
        }
        // What was handed out and not heard is read again, first, by the next page.
        for slot in slots.into_iter().rev() {
            unheard.push_front(slot.path);
        }
        ended
    }
}

/// How much of a file reaches the matcher at once: what one fill matches before stop and the
/// page's time are asked again.
pub const FILL: usize = 16 * 1024;

/// The longest line searched, in bytes. A file with a longer line — a minified bundle, or a
/// file written to be slow — is not searched, and the answer says so: the matcher works a line
/// at a time, and one line is what it cannot be stopped inside.
pub const LONGEST_LINE: usize = 256 * 1024;

/// What a file's matching is asked to hear.
struct Watch<'a> {
    stop: &'a AtomicBool,
    /// Raised when the page has ended without this file: cut as by a stop.
    over: &'a AtomicBool,
    until: Instant,
}

/// Why a file's matching was cut short.
enum Cut {
    Stop,
    /// The page's time ran out inside the file named.
    Time(String),
}

/// What reading one file found.
enum Read {
    Hits(FileHits),
    Nothing,
    NotSearched(String, String),
    Cut(Cut),
}

/// A file's bytes handed to the matcher a [`FILL`] at a time, each fill refused once the stop
/// is raised or the page's time is spent.
struct Watched<'a> {
    bytes: &'a [u8],
    at: usize,
    watch: &'a Watch<'a>,
    cut: Option<bool>,
}

impl std::io::Read for Watched<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.watch.stop.load(Ordering::Relaxed) || self.watch.over.load(Ordering::Relaxed) {
            self.cut = Some(true);
            return Err(std::io::Error::other("stopped"));
        }
        if Instant::now() >= self.watch.until {
            self.cut = Some(false);
            return Err(std::io::Error::other("out of time"));
        }
        let rest = &self.bytes[self.at..];
        let n = rest.len().min(buf.len()).min(FILL);
        buf[..n].copy_from_slice(&rest[..n]);
        self.at += n;
        Ok(n)
    }
}

/// The matches of one file, opened from the branch's held folder one component at a time,
/// following no link ([`super::open_inside`]): nothing for a file with none, or one that is binary, an
/// image, past the preview's size, not a regular file, or gone.
fn file_hits(
    matcher: &RegexMatcher,
    at: usize,
    held: &super::Held,
    path: String,
    watch: &Watch<'_>,
) -> Read {
    #[cfg(test)]
    if path.ends_with(tests::READER_PANICS) {
        panic!("a reader panicked, as the test asked");
    }
    let Ok(mut file) = super::open_inside(held, Path::new(&path)) else {
        return Read::Nothing;
    };
    let Ok(meta) = file.metadata() else {
        return Read::Nothing;
    };
    if !meta.is_file() || meta.len() > super::LARGEST {
        return Read::Nothing;
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    if file
        .by_ref()
        .take(super::LARGEST + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > super::LARGEST
        || super::image_kind(&bytes).is_some()
        || bytes[..bytes.len().min(super::SNIFF)].contains(&0)
    {
        return Read::Nothing;
    }
    let mut count = 0u64;
    let mut lines = Vec::new();
    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .binary_detection(BinaryDetection::none())
        .heap_limit(Some(LONGEST_LINE))
        .build();
    let mut reader = Watched {
        bytes: &bytes,
        at: 0,
        watch,
        cut: None,
    };
    let searched = searcher.search_reader(
        matcher,
        &mut reader,
        Lossy(|number, line: &str| {
            count += 1;
            if lines.len() < FILE_LINES {
                lines.push(hit_line(matcher, number, line));
            }
            Ok(true)
        }),
    );
    match (searched, reader.cut) {
        (_, Some(true)) => Read::Cut(Cut::Stop),
        (_, Some(false)) => Read::Cut(Cut::Time(path)),
        (Err(_), None) => Read::NotSearched(
            path,
            format!("it has a line longer than {} KiB", LONGEST_LINE / 1024),
        ),
        (Ok(()), None) if count == 0 => Read::Nothing,
        (Ok(()), None) => Read::Hits(FileHits {
            at,
            path,
            count,
            lines,
        }),
    }
}

/// A branch's folder, its offered list, and a walk of it, ready to search.
///
/// **git's offered list is the only rule.** The walk reads no ignore file of its own — no
/// `.gitignore`, no `info/exclude`, no global excludes, no `.ignore` — because each is a file
/// an agent can plant as a link to a device or a FIFO, and reading one would follow it. The
/// walk descends only into a folder that holds an offered path, so `target/` and
/// `node_modules/` are never walked, and it opens nothing but folders.
///
/// **The listing hears `stop`** (#1137): `Ok(None)` when it was raised while git listed.
fn walking(
    reader: Option<&super::Reader>,
    plane: &Path,
    named: &Named,
    stop: &AtomicBool,
) -> Result<Option<Walking>, String> {
    let branch = named.branch();
    let ready = || -> Result<Option<Walking>, Refused> {
        let folder = super::found(reader, plane, branch)?;
        let Some(offered) = super::files_in_until(&folder, branch, stop)? else {
            return Ok(None);
        };
        let offered = std::sync::Arc::new(offered);
        let unreadable = |e: std::io::Error| Refused::Unreadable {
            what: branch.called().to_string(),
            why: e.to_string(),
        };
        // **Held from the project's root, one folder at a time, following no link**: an
        // ancestor of the branch's folder swapped for a link — `.worktrees/<repo>` sits in a
        // workspace its chats write — is refused here, or, swapped later, never reached, since
        // every file is opened relative to the folder held now.
        let root = std::fs::canonicalize(plane).map_err(unreadable)?;
        let base = std::fs::canonicalize(&folder).map_err(unreadable)?;
        let held = super::hold_folder(&root, &base).map_err(unreadable)?;
        let top = base.clone();
        let listed = std::sync::Arc::clone(&offered);
        let walk = ignore::WalkBuilder::new(&base)
            .hidden(false)
            .ignore(false)
            .git_ignore(false)
            .git_exclude(false)
            .git_global(false)
            .parents(false)
            .follow_links(false)
            .same_file_system(true)
            .max_filesize(Some(super::LARGEST))
            .sort_by_file_name(|a, b| a.cmp(b))
            .filter_entry(move |entry| {
                if entry.depth() == 0 {
                    return true;
                }
                if entry.file_name() == ".git" {
                    return false;
                }
                if !entry.file_type().is_some_and(|kind| kind.is_dir()) {
                    return true;
                }
                // A repository nested in the branch is its own, not the branch's.
                if std::fs::symlink_metadata(entry.path().join(".git")).is_ok() {
                    return false;
                }
                entry
                    .path()
                    .strip_prefix(&top)
                    .is_ok_and(|inside| holds_offered(&listed, &super::slashed(inside)))
            })
            .build();
        Ok(Some(Walking {
            base,
            held,
            offered,
            walk,
            unheard: VecDeque::new(),
        }))
    };
    ready().map_err(|refused| refused.to_string())
}

/// Whether any path of the sorted `offered` list lies under the folder `folder` — as spelled,
/// or composed as git spells a name where the system composes (macOS).
fn holds_offered(offered: &[String], folder: &str) -> bool {
    use unicode_normalization::UnicodeNormalization as _;
    let under = |folder: &str| {
        let prefix = format!("{folder}/");
        let from = offered.partition_point(|one| one.as_str() < prefix.as_str());
        offered
            .get(from)
            .is_some_and(|one| one.starts_with(&prefix))
    };
    under(folder) || under(&folder.nfc().collect::<String>())
}

/// The next file of `walk` the search reads, as [`searchable`] says; `None` once the walk is
/// done.
fn next_searchable(walk: &mut ignore::Walk, base: &Path, offered: &[String]) -> Option<String> {
    for entry in walk.by_ref() {
        let Ok(entry) = entry else { continue };
        if let Some(path) = searchable(base, offered, &entry) {
            return Some(path);
        }
    }
    None
}

/// The path, relative to the branch and as git's list spells it, of an entry the search reads;
/// `None` for anything it does not.
fn searchable(base: &Path, offered: &[String], entry: &ignore::DirEntry) -> Option<String> {
    use unicode_normalization::UnicodeNormalization as _;
    // Never a link, whatever it leads to, and never a folder, a FIFO or a socket.
    if entry.path_is_symlink() || !entry.file_type().is_some_and(|kind| kind.is_file()) {
        return None;
    }
    let relative = entry.path().strip_prefix(base).ok()?;
    if super::names_git(entry.path(), base) {
        return None;
    }
    let path = super::slashed(relative);
    // git lists a name composed where the system composes (macOS), whatever the disk holds.
    let composed: String = path.nfc().collect();
    let offers = |one: &str| offered.binary_search_by(|o| o.as_str().cmp(one)).is_ok();
    if !offers(&path) && !offers(&composed) {
        return None;
    }
    if guarded(&path) {
        return None;
    }
    Some(path)
}

/// Whether a path is one search never reads, whatever git says of it: in a vault or charter's
/// own state, or named like a credential.
fn guarded(path: &str) -> bool {
    if crate::leakguard::vault_path_matches(path)
        || path
            .split('/')
            .any(|step| crate::leakguard::vault_path_matches(&format!("{step}/")))
    {
        return true;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    crate::reposave::secret_name(name).is_some() || lower == ".npmrc" || lower == ".pypirc"
}

/// A matching line as [`Part`]s, its line ending dropped and, past [`LINE_CHARS`], clipped to
/// a stretch around its first match.
fn hit_line(matcher: &RegexMatcher, number: u64, line: &str) -> HitLine {
    let line = line.trim_end_matches(['\n', '\r']);
    let mut found: Vec<(usize, usize)> = Vec::new();
    let _ = matcher.find_iter(line.as_bytes(), |m| {
        // An empty match (`^`, `\b`) marks nothing.
        if m.start() < m.end() && line.is_char_boundary(m.start()) && line.is_char_boundary(m.end())
        {
            found.push((m.start(), m.end()));
        }
        true
    });
    let (from, to, clipped) = window(line, found.first().map_or(0, |first| first.0));
    let mut parts = Vec::new();
    let mut said = from;
    for (start, end) in found {
        let (start, end) = (start.max(from), end.min(to));
        if start >= end {
            continue;
        }
        if said < start {
            parts.push(Part {
                text: line[said..start].to_string(),
                hit: false,
            });
        }
        parts.push(Part {
            text: line[start..end].to_string(),
            hit: true,
        });
        said = end;
    }
    if said < to {
        parts.push(Part {
            text: line[said..to].to_string(),
            hit: false,
        });
    }
    HitLine {
        number,
        parts,
        clipped,
    }
}

/// The stretch of `line` kept, as byte offsets on character boundaries: all of it when it is
/// within [`LINE_CHARS`], otherwise that many characters from a little before `first`.
fn window(line: &str, first: usize) -> (usize, usize, bool) {
    if line.chars().count() <= LINE_CHARS {
        return (0, line.len(), false);
    }
    let lead = LINE_CHARS / 4;
    let before: Vec<usize> = line[..first].char_indices().map(|(at, _)| at).collect();
    let from = before
        .len()
        .checked_sub(lead)
        .map_or(0, |skip| before[skip]);
    let to = line[from..]
        .char_indices()
        .nth(LINE_CHARS)
        .map_or(line.len(), |(at, _)| from + at);
    (from, to, from > 0 || to < line.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Branch, Place};

    /// #1189: a search reading with a reader finds each branch's folder through it: a branch
    /// the reader refuses is refused in the reader's own sentence.
    #[cfg(unix)]
    #[test]
    fn a_search_reading_with_a_reader_finds_each_folder_through_it() {
        let nowhere = tempfile::tempdir().unwrap();
        let said = "thing in workspace 'alpha' has no branch folder called 'piece'";
        let answer = serde_json::json!({ "Err": said }).to_string();
        let (_answer, reader) = crate::files::tests::reader_answering(&answer);
        let scope = [Place {
            plane: nowhere.path(),
            branch: Branch::piece("alpha", "thing", "piece"),
        }];
        let mut heard = Vec::new();

        let ended = search(&scope, "needle", SearchOptions::default())
            .unwrap()
            .reading_with(reader)
            .more(10, &AtomicBool::new(false), &mut |one| heard.push(one));

        assert_eq!(ended, Ended::Done);
        assert_eq!(
            heard,
            [Searched::Refused {
                at: 0,
                why: said.to_string()
            }]
        );
    }

    /// A file whose name ends so makes its reader panic.
    pub(super) const READER_PANICS: &str = "a-reader-panics-here.txt";

    /// #1153 review: a reader that panics must not leave the page waiting on it for ever. The
    /// panic reaches the caller, and the page ends at once.
    #[test]
    fn a_reader_that_panics_ends_the_page_with_its_panic_and_nothing_hangs() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
        let clone = plane.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).unwrap();
        let ran = crate::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&clone)
                .args(["init", "-q"])
                .env("GIT_CONFIG_GLOBAL", "/dev/null"),
        )
        .unwrap();
        assert!(ran.status.success(), "{ran:?}");
        for n in 0..40 {
            std::fs::write(clone.join(format!("f{n:02}.txt")), "needle\n").unwrap();
        }
        std::fs::write(clone.join(format!("f20-{READER_PANICS}")), "needle\n").unwrap();

        let searching = std::thread::spawn(move || {
            let scope = [Place {
                plane: &plane,
                branch: Branch::repo("alpha", "thing"),
            }];
            let mut search = search(&scope, "needle", SearchOptions::default()).unwrap();
            let stop = AtomicBool::new(false);
            search.more(usize::MAX, &stop, &mut |_| {})
        });
        let started = Instant::now();
        while !searching.is_finished() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "the page hung on a reader that panicked"
            );
            std::thread::sleep(Duration::from_millis(20));
        }

        let panicked = searching
            .join()
            .expect_err("the reader's panic reaches the caller");
        let said = panicked
            .downcast_ref::<&str>()
            .map(|said| said.to_string())
            .or_else(|| panicked.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        assert!(said.contains("a reader panicked"), "{said:?}");
        drop(dir);
    }

    /// #1137: a branch's listing hears the page's stop. A `.gitignore` that is a FIFO holds
    /// `git ls-files --exclude-standard` in its open until the git deadline (30 s); a raised
    /// stop ends that git and the page answers `Stopped` at once, keeping its place.
    #[cfg(unix)]
    #[test]
    fn a_stop_ends_a_branch_listing_held_on_a_fifo_ignore_file() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
        let clone = plane.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).unwrap();
        let ran = crate::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&clone)
                .args(["init", "-q"])
                .env("GIT_CONFIG_GLOBAL", "/dev/null"),
        )
        .unwrap();
        assert!(ran.status.success(), "{ran:?}");
        std::fs::write(clone.join("a.txt"), "needle\n").unwrap();
        let made = crate::forklock::output(
            std::process::Command::new("mkfifo").arg(clone.join(".gitignore")),
        )
        .unwrap();
        assert!(made.status.success(), "{made:?}");

        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let raised = std::sync::Arc::clone(&stop);
        let searching = std::thread::spawn(move || {
            let scope = [Place {
                plane: &plane,
                branch: Branch::repo("alpha", "thing"),
            }];
            let mut search = search(&scope, "needle", SearchOptions::default()).unwrap();
            let mut refused = Vec::new();
            let ended = search.more(usize::MAX, &stop, &mut |heard| {
                if let Searched::Refused { why, .. } = heard {
                    refused.push(why);
                }
            });
            (ended, refused)
        });
        // Long enough for git to be held in the FIFO's open.
        std::thread::sleep(Duration::from_millis(500));
        let raised_at = Instant::now();
        raised.store(true, Ordering::Relaxed);
        while !searching.is_finished() {
            assert!(
                raised_at.elapsed() < Duration::from_secs(10),
                "the listing ran on past the stop"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let (ended, refused) = searching.join().unwrap();
        // The stop asks git first and kills it only after its grace, so a second's answer
        // plus that grace is the bound, never the 30 s deadline.
        assert!(
            raised_at.elapsed() < Duration::from_secs(1) + Duration::from_secs(2),
            "{:?}",
            raised_at.elapsed()
        );
        assert_eq!(ended, Ended::Stopped);
        assert!(refused.is_empty(), "a stop is not a refusal: {refused:?}");
        drop(dir);
    }
}
