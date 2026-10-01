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
//! - **And at most `max_age` old**, the backstop for a working-tree edit nothing reported: the
//!   plane's watcher is not recursive, by design (`planewatch.rs`: a recursive inotify watch on
//!   a monorepo runs the machine out of watches), so an edit deep in a clone is seen within
//!   `max_age`, which is how stale the title bar already was between its reads.
//!
//! In memory and in this process only: nothing is written, so there is no store to give a tier
//! (ADR 0069). It is plain core code, so the host that ADR 0068 moves auto-save and the watcher
//! into owns it there as the app does here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

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
}

struct Kept<V> {
    value: V,
    at: Instant,
    generation: u64,
    stamp: Stamp,
}

impl<V> Default for State<V> {
    fn default() -> Self {
        Self {
            kept: None,
            generation: 0,
            computing: false,
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
                && kept.at.elapsed() < self.max_age
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
        let value = compute();
        let mut state = slot.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.kept = Some(Kept {
            value: value.clone(),
            at: Instant::now(),
            generation,
            stamp,
        });
        drop(state);
        drop(done);
        value
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

/// The repo's git files as they are, by size, modification time and inode: what moves when a
/// commit, a checkout, a fetch, a staged change, a merge or a rebase moves the repo. Read with
/// `stat` and one small read of `HEAD`, never with git.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stamp(Vec<Option<(u64, u128, u64)>>);

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
        let mut files: Vec<PathBuf> = [
            "HEAD",
            "index",
            "FETCH_HEAD",
            "ORIG_HEAD",
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
        ]
        .iter()
        .map(|name| git.join(name))
        .collect();
        files.push(common.join("packed-refs"));
        // The branch HEAD is on, and the remote-tracking branch it is counted against.
        if let Some(branch) = std::fs::read_to_string(git.join("HEAD"))
            .ok()
            .and_then(|head| {
                head.strip_prefix("ref: refs/heads/")
                    .map(|b| b.trim().to_owned())
            })
        {
            files.push(common.join("refs/heads").join(&branch));
            files.push(common.join("refs/remotes/origin").join(&branch));
        }
        Self(files.iter().map(|path| seen(path)).collect())
    }
}

fn seen(path: &Path) -> Option<(u64, u128, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let at = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&meta);
    #[cfg(not(unix))]
    let inode = 0;
    Some((meta.len(), at, inode))
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
}
