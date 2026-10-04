//! One standing per repo, shared by everything that asks (FD-11, #651).
//!
//! Four loops asked git where a repo stood, each on its own clock: auto-save's look every two
//! seconds, the title bar's and the Saving tab's every ten, and the alerts' every sixty. About
//! ten git processes every two seconds per plane, each poller paying for the same answer, and in
//! a large repo — where one `status` outlasts the next tick — several of them running in the
//! repo at once. [`Shared`] is the one answer they share now:
//!
//! - **Single-flight per repo.** While one caller computes, the others wait for its answer;
//!   two computations never run in one repo at once.
//! - **Recomputed on change, not on a clock.** An answer is current while the repo's git files
//!   are as they were ([`Stamp`]: `HEAD`, the index, the branch's refs, a fetch, a merge or a
//!   rebase moving), and nobody has [`Shared::touch`]ed the repo — the plane's watcher on every
//!   batch, a save or a fetch letting go of its claim, a chat ending.
//! - **Watched, where a watch covers the working tree.** The app's one watch per repo
//!   (`branchwatch.rs`) listens to a clone's whole working tree, asks whether what moved is
//!   anything git would show, and [`Shared::touch`]es it when it is; while it covers a repo
//!   ([`Shared::cover`]) the repo's answer is read again on what moved, and otherwise only after
//!   [`WATCHED_FOR`] or [`WATCHED_SHARE`] times what its last read took, whichever is longer:
//!   the backstop for a watch that went quiet without saying so. A monorepo nobody touches
//!   costs about one `status` in twenty minutes. Where to watch is asked of whoever
//!   [`watch_with`] names ([`want_watched`]); with nobody named — the CLI — nothing is watched.
//! - **And at most `max_age` old where nothing watches**, the backstop for a working-tree edit
//!   nothing reported: the plane's own watcher is not recursive, by design (`planewatch.rs`: a
//!   recursive inotify watch on a monorepo runs the machine out of watches), and on inotify a
//!   clone past the app's share of the watches is not covered either.
//! - **Never more often than the idle budget allows.** However short the backstop, an answer
//!   is kept for at least [`IDLE_SHARE`] times what it last took to read, so reading a repo
//!   no watch covers again on a clock takes at most 1/[`IDLE_SHARE`] of the time: a monorepo
//!   whose `status` takes a second is read every few minutes, not every ten seconds.
//!
//! In memory and in this process only: nothing is written, so there is no store to give a tier
//! (ADR 0069). It is plain core code, so the host that ADR 0068 moves auto-save and the watcher
//! into owns it there as the app does here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

/// How long an answer for a repo a watch covers is kept, at least, while the watch says nothing
/// moved: the backstop for a watch that stopped hearing without an error (D-88l's rule for the
/// kill switch: a dead watch means a slower look, never none).
pub const WATCHED_FOR: Duration = Duration::from_secs(300);

/// A covered repo's answer is kept for at least this many times what its read took. Measured
/// at 300,000 files, a `status` took 1.2 s of wall time and about 6 s of CPU across git's
/// threads, so a thousandth of the wall time is the half a percent of a core SC-18's idle
/// budget allows (V8): one read in twenty minutes.
pub const WATCHED_SHARE: u32 = 1000;

/// A repo no watch covers has its answer kept for at least this many times what its read
/// took, past the store's own backstop: a clock that never takes more than 1/200 of the wall
/// time. Lower than [`WATCHED_SHARE`] because nothing else tells this repo's edits; a
/// `status` reading on several threads can take a few times that in CPU.
pub const IDLE_SHARE: u32 = 200;

/// Each repo's last answer, shared by every caller.
pub struct Shared<V> {
    slots: Mutex<HashMap<PathBuf, Arc<Slot<V>>>>,
    max_age: Duration,
}

struct Slot<V> {
    state: Mutex<State<V>>,
    ready: Condvar,
}

struct State<V> {
    /// The last answer: what it was, when it was finished, the generation it was started at and
    /// the repo's git files as they were when it started.
    kept: Option<Kept<V>>,
    /// Bumped by [`Shared::touch`]: an answer started at an older generation is not current.
    generation: u64,
    /// One computation is running, and every other caller waits for it.
    computing: bool,
    /// A watch covers the repo's working tree and touches it when something there moves.
    covered: bool,
}

struct Kept<V> {
    value: V,
    at: Instant,
    /// How long computing it took.
    cost: Duration,
    generation: u64,
    stamp: Stamp,
}

impl<V> Default for State<V> {
    fn default() -> Self {
        Self {
            kept: None,
            generation: 0,
            computing: false,
            covered: false,
        }
    }
}

impl<V: Clone> Shared<V> {
    /// A store whose answers are kept for at most `max_age`: the backstop for a change in a
    /// working tree that nothing reported.
    pub fn new(max_age: Duration) -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            max_age,
        }
    }

    fn slot(&self, repo: &Path) -> Arc<Slot<V>> {
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(slots.entry(repo.to_path_buf()).or_insert_with(|| {
            Arc::new(Slot {
                state: Mutex::new(State::default()),
                ready: Condvar::new(),
            })
        }))
    }

    /// The standing of `repo`: the last answer while it is current, else `compute`'s.
    ///
    /// **Single-flight**: while one caller computes, every other caller for the same repo
    /// waits for that answer instead of starting git again. An answer is current while
    /// nothing touched the repo since it started, the repo's git files ([`Stamp`]) are as they
    /// were, and it is younger than the store's `max_age`.
    pub fn get(&self, repo: &Path, compute: impl FnOnce() -> V) -> V {
        let slot = self.slot(repo);
        let mut state = slot.state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(kept) = &state.kept
                && kept.generation == state.generation
                && kept.at.elapsed() < self.kept_for(state.covered, kept.cost)
                && kept.stamp == Stamp::of(repo)
            {
                return kept.value.clone();
            }
            if !state.computing {
                break;
            }
            state = slot
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.computing = true;
        let generation = state.generation;
        drop(state);
        // Taken before git reads the repo, so a change made while it reads is a change the
        // next caller sees.
        let stamp = Stamp::of(repo);
        let done = Done { slot: &slot };
        let started = Instant::now();
        let value = compute();
        let mut state = slot.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.kept = Some(Kept {
            value: value.clone(),
            at: Instant::now(),
            cost: started.elapsed(),
            generation,
            stamp,
        });
        drop(state);
        drop(done);
        value
    }

    /// How long an answer that took `cost` is kept without anything saying the repo moved.
    fn kept_for(&self, covered: bool, cost: Duration) -> Duration {
        if covered {
            WATCHED_FOR.max(cost.saturating_mul(WATCHED_SHARE))
        } else {
            self.max_age.max(cost.saturating_mul(IDLE_SHARE))
        }
    }

    /// Whether a watch covers `repo`'s working tree now ([`Shared::cover`]).
    pub fn covered(&self, repo: &Path) -> bool {
        self.slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(repo)
            .is_some_and(|slot| {
                slot.state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .covered
            })
    }

    /// A watch now covers `repo`'s whole working tree (`true`) and will [`Shared::touch`] it
    /// when anything there moves, or no longer does (`false`). Either way the repo is read
    /// again at its next ask: what moved while nothing listened, or before the watch began,
    /// is not known, and an answer read then must not be kept for a covered repo's backstop.
    pub fn cover(&self, repo: &Path, covered: bool) {
        let slot = self.slot(repo);
        let mut state = slot.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.covered != covered {
            state.generation += 1;
        }
        state.covered = covered;
    }

    /// Something changed in `repo`: the next [`Shared::get`] computes again, and an answer
    /// being computed now is not kept as current.
    pub fn touch(&self, repo: &Path) {
        self.touch_where(|key| key == repo);
    }

    /// [`Shared::touch`] for every repo at or under `dir`: a plane's settings, which every
    /// clone in it reads, or a watcher that saw the plane move.
    pub fn touch_within(&self, dir: &Path) {
        self.touch_where(|key| key.starts_with(dir));
    }

    fn touch_where(&self, which: impl Fn(&Path) -> bool) {
        let touched: Vec<Arc<Slot<V>>> = self
            .slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(key, _)| which(key))
            .map(|(_, slot)| Arc::clone(slot))
            .collect();
        for slot in touched {
            slot.state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .generation += 1;
        }
    }
}

/// A clone whose standing is shared and that no watch covers yet: what the app needs to find
/// its working tree the way the explorer does (`charter_core::files::root`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub plane: PathBuf,
    pub workspace: String,
    pub repo: String,
    /// The clone's folder, as the standing reads it.
    pub path: PathBuf,
}

/// Who is asked to watch a clone whose standing is shared ([`want_watched`]).
pub type Watching = Arc<dyn Fn(&Wanted) + Send + Sync>;

static WATCHING: RwLock<Option<Watching>> = RwLock::new(None);

/// Names who watches the working trees of the clones whose standing is shared: the app, which
/// holds the one watch per repo. Until it is named — and in the CLI, which never names one —
/// nothing is watched and every standing keeps its clock.
pub fn watch_with(watching: Watching) {
    *WATCHING.write().unwrap_or_else(PoisonError::into_inner) = Some(watching);
}

/// Asks whoever [`watch_with`] named to watch `wanted`'s working tree. It is asked on every
/// shared read of a clone no watch covers yet, and answers at once: the watch is set up on a
/// thread of its own, and covers the clone ([`Shared::cover`]) once it is listening.
pub fn want_watched(wanted: &Wanted) {
    let watching = WATCHING
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(watching) = watching {
        watching(wanted);
    }
}

/// Ends a computation, however it ends: a `compute` that panics must not leave every later
/// caller waiting for it.
struct Done<'a, V> {
    slot: &'a Slot<V>,
}

impl<V> Drop for Done<'_, V> {
    fn drop(&mut self) {
        let mut state = self
            .slot
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.computing = false;
        drop(state);
        self.slot.ready.notify_all();
    }
}

/// The repo's git state as the file system describes it: what moves when a commit, a
/// checkout, a fetch, a staged change, a merge, a rebase, a ref of any name, the repo's config or
/// its top ignore files move it. Read with `stat` and one small read of `commondir`, never
/// with git.
///
/// - `HEAD`, the index, `FETCH_HEAD`, `ORIG_HEAD`, the merge, cherry-pick, revert and rebase
///   state, and `config` (a `[branch]` upstream, `core.untrackedCache`);
/// - every directory under `refs/`, by its modification time, which a ref created, moved or
///   deleted in it changes — so a branch in a folder, an upstream that is not `origin` and a
///   `[plane] branch` HEAD is not on are all in it — and `packed-refs`;
/// - a reftable repo's `reftable/tables.list`, which git rewrites on every ref update;
/// - `.gitignore` at the repo's top and `info/exclude`.
///
/// **Not in it, and left to the store's `max_age`:** a change in the working tree nothing
/// reported, an ignore file below the top, a global excludes file or config, and a linked
/// worktree's own refs (`refs/bisect`, `refs/worktree` under its private directory).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stamp(Vec<Option<Seen>>);

/// One file or directory as `stat` describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub size: u64,
    /// Nanoseconds since the epoch.
    pub modified: u128,
    pub inode: u64,
}

impl Stamp {
    /// The stamp of the repo whose top is `repo`.
    pub fn of(repo: &Path) -> Self {
        let Some(git) = git_dir(repo) else {
            return Self::default();
        };
        let common = std::fs::read_to_string(git.join("commondir"))
            .ok()
            .map(|to| git.join(to.trim()))
            .unwrap_or_else(|| git.clone());
        let mut paths: Vec<PathBuf> = [
            "HEAD",
            "index",
            "FETCH_HEAD",
            "ORIG_HEAD",
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            // A linked worktree's own config; the shared `config` is the common directory's,
            // below, which in a plain repo is this same directory.
            "config.worktree",
        ]
        .iter()
        .map(|name| git.join(name))
        .collect();
        paths.extend(
            [
                "packed-refs",
                "config",
                "info/exclude",
                "reftable/tables.list",
            ]
            .iter()
            .map(|name| common.join(name)),
        );
        paths.push(repo.join(".gitignore"));
        let mut dirs = Vec::new();
        directories(&common.join("refs"), &mut dirs);
        paths.extend(dirs);
        Self(paths.iter().map(|path| seen(path)).collect())
    }
}

/// `dir` and every directory under it, in name order, without following a link. A deep tree of
/// branches is a few hundred directories at most; the files in them are not asked about.
fn directories(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    out.push(dir.to_path_buf());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut inside: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    inside.sort();
    for one in inside {
        directories(&one, out);
    }
}

fn seen(path: &Path) -> Option<Seen> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&meta);
    #[cfg(not(unix))]
    let inode = 0;
    Some(Seen {
        size: meta.len(),
        modified,
        inode,
    })
}

/// The repository directory of the repo whose top is `repo`: `.git`, or where a linked
/// worktree's `.git` file points.
fn git_dir(repo: &Path) -> Option<PathBuf> {
    let dot = repo.join(".git");
    let meta = std::fs::symlink_metadata(&dot).ok()?;
    if meta.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let to = text.strip_prefix("gitdir:")?.trim();
    Some(repo.join(to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A computation that takes `for_ms`, counts how often it ran, and records the most that
    /// ran at once.
    struct Counted {
        ran: AtomicUsize,
        now: AtomicUsize,
        most: AtomicUsize,
    }

    impl Counted {
        fn new() -> Self {
            Self {
                ran: AtomicUsize::new(0),
                now: AtomicUsize::new(0),
                most: AtomicUsize::new(0),
            }
        }

        fn run(&self, for_ms: u64) -> usize {
            let at_once = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(at_once, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(for_ms));
            self.now.fetch_sub(1, Ordering::SeqCst);
            self.ran.fetch_add(1, Ordering::SeqCst) + 1
        }
    }

    fn a_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("a directory")
    }

    #[test]
    fn callers_at_once_share_one_computation_and_never_overlap() {
        let shared = Shared::new(Duration::from_secs(60));
        let counted = Counted::new();
        let repo = a_dir();

        let answers: Vec<usize> = std::thread::scope(|scope| {
            let all: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| shared.get(repo.path(), || counted.run(100))))
                .collect();
            all.into_iter()
                .map(|one| one.join().expect("a caller"))
                .collect()
        });

        assert_eq!(counted.most.load(Ordering::SeqCst), 1, "two ran at once");
        assert_eq!(counted.ran.load(Ordering::SeqCst), 1, "{answers:?}");
        assert!(answers.iter().all(|one| *one == 1), "{answers:?}");
    }

    #[test]
    fn a_second_ask_with_nothing_changed_computes_nothing() {
        let shared = Shared::new(Duration::from_secs(60));
        let counted = Counted::new();
        let repo = a_dir();

        shared.get(repo.path(), || counted.run(0));
        shared.get(repo.path(), || counted.run(0));

        assert_eq!(counted.ran.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_touch_makes_the_next_ask_compute_again() {
        let shared = Shared::new(Duration::from_secs(60));
        let counted = Counted::new();
        let repo = a_dir();

        shared.get(repo.path(), || counted.run(0));
        shared.touch(repo.path());
        let again = shared.get(repo.path(), || counted.run(0));

        assert_eq!(again, 2);
    }

    #[test]
    fn an_answer_older_than_the_longest_it_is_kept_is_computed_again() {
        let shared = Shared::new(Duration::from_millis(30));
        let counted = Counted::new();
        let repo = a_dir();

        shared.get(repo.path(), || counted.run(0));
        std::thread::sleep(Duration::from_millis(60));
        let again = shared.get(repo.path(), || counted.run(0));

        assert_eq!(again, 2);
    }

    #[test]
    fn two_repos_are_two_answers() {
        let shared = Shared::new(Duration::from_secs(60));
        let (one, two) = (a_dir(), a_dir());

        assert_eq!(shared.get(one.path(), || 1), 1);
        assert_eq!(shared.get(two.path(), || 2), 2);
        assert_eq!(shared.get(one.path(), || 3), 1);
    }

    #[test]
    fn a_touch_while_computing_is_not_lost() {
        // A change that lands while git is reading the repo may not be in what it read, so the
        // answer that read produces is not kept as current.
        let shared = Shared::new(Duration::from_secs(60));
        let repo = a_dir();

        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                shared.get(repo.path(), || {
                    std::thread::sleep(Duration::from_millis(100));
                    1
                })
            });
            std::thread::sleep(Duration::from_millis(30));
            shared.touch(repo.path());
            assert_eq!(first.join().expect("the first"), 1);
        });

        assert_eq!(shared.get(repo.path(), || 2), 2);
    }

    #[test]
    fn a_watched_repos_answer_outlives_the_backstop_until_the_watch_says_it_moved() {
        // FD-11: a repo whose working tree is watched is read again when the watch says so,
        // and otherwise only on its long backstop: a monorepo nobody touches costs about one
        // `status` in twenty minutes.
        let shared = Shared::new(Duration::from_millis(30));
        let counted = Counted::new();
        let repo = a_dir();
        shared.cover(repo.path(), true);

        shared.get(repo.path(), || counted.run(0));
        std::thread::sleep(Duration::from_millis(60));
        let idle = shared.get(repo.path(), || counted.run(0));
        shared.touch(repo.path());
        let moved = shared.get(repo.path(), || counted.run(0));

        assert_eq!((idle, moved), (1, 2));
        assert!(shared.covered(repo.path()));
    }

    #[test]
    fn a_watch_that_begins_has_the_repo_read_again() {
        // An answer read before the watch was listening may already miss an edit the watch
        // will never report; it is not kept for a covered repo's long backstop.
        let shared = Shared::new(Duration::from_secs(60));
        let counted = Counted::new();
        let repo = a_dir();
        shared.get(repo.path(), || counted.run(0));

        shared.cover(repo.path(), true);
        let again = shared.get(repo.path(), || counted.run(0));

        assert_eq!(again, 2);
    }

    #[test]
    fn a_watch_that_ends_has_the_repo_read_again() {
        // What moved while nothing watched is not known: the next ask reads.
        let shared = Shared::new(Duration::from_secs(60));
        let counted = Counted::new();
        let repo = a_dir();
        shared.cover(repo.path(), true);
        shared.get(repo.path(), || counted.run(0));

        shared.cover(repo.path(), false);
        let again = shared.get(repo.path(), || counted.run(0));

        assert_eq!(again, 2);
        assert!(!shared.covered(repo.path()));
    }

    #[test]
    fn an_unwatched_repo_whose_read_is_costly_is_read_again_only_within_the_idle_budget() {
        // A repo no watch covers is read again on the clock, but never more often than keeps
        // the reads to 1/IDLE_SHARE of the time: a read of 20 ms waits at least four seconds,
        // however short the backstop.
        let shared = Shared::new(Duration::from_millis(30));
        let counted = Counted::new();
        let repo = a_dir();

        shared.get(repo.path(), || counted.run(20));
        std::thread::sleep(Duration::from_millis(60));
        let again = shared.get(repo.path(), || counted.run(20));

        assert_eq!(again, 1);
    }

    /// A repo with one commit, through `testgit`, so no developer's signer is asked.
    fn a_repo(extra: &[&str]) -> tempfile::TempDir {
        let dir = a_dir();
        let init: Vec<&str> = ["init", "-q", "-b", "main"]
            .into_iter()
            .chain(extra.iter().copied())
            .chain(["."])
            .collect();
        for argv in [
            init,
            vec!["config", "user.email", "t@e.invalid"],
            vec!["config", "user.name", "t"],
            vec!["commit", "-q", "--allow-empty", "-m", "one"],
        ] {
            assert!(crate::testgit::run(dir.path(), &argv).ok(), "git {argv:?}");
        }
        dir
    }

    fn git(dir: &Path, argv: &[&str]) {
        assert!(crate::testgit::run(dir, argv).ok(), "git {argv:?}");
    }

    /// The stamp moves when `change` changes the repo.
    fn moves(repo: &Path, change: impl FnOnce()) -> bool {
        let before = Stamp::of(repo);
        // Past a coarse filesystem clock, so a same-size rewrite is still a new time.
        std::thread::sleep(Duration::from_millis(20));
        change();
        Stamp::of(repo) != before
    }

    #[test]
    fn the_stamp_moves_with_a_commit_and_the_index() {
        let repo = a_repo(&[]);
        assert!(moves(repo.path(), || git(
            repo.path(),
            &["commit", "-q", "--allow-empty", "-m", "two"]
        )));
        std::fs::write(repo.path().join("a"), "a").unwrap();
        assert!(moves(repo.path(), || git(repo.path(), &["add", "a"])));
    }

    #[test]
    fn the_stamp_moves_with_any_ref_however_it_is_named() {
        let repo = a_repo(&[]);
        // An upstream that is not `origin`, a branch in a folder, and a branch HEAD is not on —
        // a `[plane] branch` other than the one checked out.
        assert!(moves(repo.path(), || git(
            repo.path(),
            &["update-ref", "refs/remotes/upstream/main", "HEAD"]
        )));
        assert!(moves(repo.path(), || git(
            repo.path(),
            &["update-ref", "refs/heads/team/feature", "HEAD"]
        )));
        assert!(moves(repo.path(), || git(
            repo.path(),
            &["branch", "-f", "release", "HEAD"]
        )));
    }

    #[test]
    fn the_stamp_moves_with_the_config_and_the_top_ignore_files() {
        let repo = a_repo(&[]);
        assert!(moves(repo.path(), || git(
            repo.path(),
            &["config", "branch.main.remote", "upstream"]
        )));
        assert!(moves(repo.path(), || std::fs::write(
            repo.path().join(".gitignore"),
            "target/\n"
        )
        .unwrap()));
        std::fs::create_dir_all(repo.path().join(".git/info")).unwrap();
        assert!(moves(repo.path(), || std::fs::write(
            repo.path().join(".git/info/exclude"),
            "scratch/\n"
        )
        .unwrap()));
    }

    #[test]
    fn the_stamp_moves_with_a_reftable_repos_refs() {
        // git 2.45 and later; an older git has no reftable to stamp.
        let probe = a_dir();
        if !crate::testgit::run(probe.path(), &["init", "-q", "--ref-format=reftable", "."]).ok() {
            return;
        }
        let repo = a_repo(&["--ref-format=reftable"]);
        assert!(moves(repo.path(), || git(
            repo.path(),
            &["update-ref", "refs/remotes/upstream/main", "HEAD"]
        )));
    }
}
