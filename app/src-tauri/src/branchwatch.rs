//! Tells a window when anything in a branch it shows the changes of moves (FM-4, #1107): an
//! agent's first write into a folder nobody opened is heard, and the branch's change markers
//! are read again without a refresh.
//!
//! **Not the tree's watch** (`filewatch.rs`), which watches exactly the folders a window has
//! open so it can read each again. This one listens to the whole branch, and says only *which
//! branch* moved: the window reads its status again.
//!
//! **What the whole branch costs depends on the platform**, so it is watched two ways
//! (`purlis_core::files::Root`):
//! - where one watch covers a tree (FSEvents on macOS, `ReadDirectoryChangesW` on Windows), the
//!   branch's folder, recursively. Adding folders one at a time there restarts the stream each
//!   time, measured at about 6 ms a folder on macOS, so a branch of 2,000 folders would take 12
//!   seconds to watch; one recursive watch takes milliseconds.
//! - where a recursive watch is one per folder (inotify), the folders git knows, each
//!   non-recursively, so `node_modules/` and `target/` cost nothing. A folder created in one is
//!   added when it appears.
//!
//! **A move that cannot change the status is not told**: one only inside what git ignores (a
//! build writing `target/`), or inside git's own folder. Asked once per branch per burst
//! (`Root::matters`), of the bounded reader (`purlis_core::files::Reader`, D-88h), and
//! **never on the watch's own thread**: each branch's check runs on a worker of its own, one at
//! a time, with what moved meanwhile asked next. A branch whose check hangs (an ignore file that
//! is a FIFO) holds only its own worker until the reader's deadline, which counts as "it
//! matters", and every other branch's markers keep moving.
//!
//! **Every burst is told by what it named** ([`crate::watchset::bursts`], #1139): a file made
//! and removed inside one is still a move. A burst that is everything — the platform lost
//! track, a watcher error, more paths than a burst holds — moves every branch listened to, and
//! may have made a folder, so a branch watched folder by folder is listed again.
//!
//! **The one watch per repo** (FD-11, #651). A clone whose save standing is shared
//! (`purlis_core::reposave::shared_standing`: auto-save's look, the Saving rows) asks to be
//! watched here too ([`BranchWatch::want`]), so the explorer's markers and the standing hear
//! one watch and one check per burst, not one each. A move that matters touches the
//! standing (`purlis_core::planegit::touch`) as well as telling the windows, and while the
//! clone's whole tree is watched its standing is *covered*: read again on what moved, and
//! otherwise only on its long backstop — about one read in twenty minutes for a monorepo
//! nobody touches (`purlis_core::standings::WATCHED_SHARE`). Its plane let go of, it is let
//! go of too.
//!
//! **Only a watch of the whole tree covers** (D-FD11h, fail closed): FSEvents and
//! `ReadDirectoryChangesW`, [`How::Whole`]. A clone watched folder by folder (inotify) is never
//! covered, and keeps the standing's clock: a folder deleted and made again, a folder made
//! between a listing and its watch, or folders that could not be listed ([`How::Unread`]) all
//! leave edits no watch reports. A burst that says the platform lost track or erred
//! ([`crate::watchset::Burst::lost`]) uncovers every clone and watches each again; a clone is
//! covered again only once that watch has been made, and its standing is read again then.
//!
//! **Nothing here walks a tree.** Nothing keeps a file-id cache, and links are not followed: a
//! link an agent puts in its branch to the operator's home is never read through.
//! On inotify, the folders a branch is listed again with as folders appear are listed at most
//! once a second, and the app watches at most a quarter of the user's inotify watches.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, Weak, mpsc};
use std::time::{Duration, Instant};

use notify::RecursiveMode;
use purlis_core::files::{Branch, Reader, Root};
use purlis_core::standings::Wanted;

use crate::planes::{PlaneId, Planes};

/// The event a window is sent.
pub(crate) const CHANGED: &str = "branch-changed";

/// How long a burst is folded for, as the tree's watch folds it.
const QUIET_FOR: Duration = Duration::from_millis(250);

/// The most moved paths one burst holds, across every branch. Past it the burst is everything
/// and every branch is read again; kept well above what one check asks
/// (`purlis_core::files::ASKED`), so a burst inside git's own folder or what it ignores is
/// sorted rather than told.
const MOST_PATHS: usize = 4096;

/// The most branches one window listens to at once.
pub const BRANCHES: usize = 32;

/// A branch, as the window names it: the plane, the workspace, the repo and the piece (none for
/// the repo's own folder).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct WatchedBranch {
    pub plane: PlaneId,
    pub workspace: String,
    pub repo: String,
    pub piece: Option<String>,
}

/// What `branch-changed` carries: the branches, of those this window listens to, that moved.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct BranchChanged {
    pub branches: Vec<WatchedBranch>,
}

/// Told which window's branches moved.
pub type Told = Arc<dyn Fn(&str, Vec<WatchedBranch>) + Send + Sync + 'static>;

/// How one branch is listened to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum How {
    /// Its folder, recursively.
    Whole,
    /// These folders, each non-recursively, refreshed as folders appear.
    Folders(Vec<PathBuf>),
    /// Folder by folder, but the folders git knows could not be listed (the reader refused or
    /// ran out of time): only the branch's own folder, non-recursively, until a listing works.
    Unread,
}

/// The platform's way: [`How::Whole`] where one watch covers a tree.
pub fn how(root: &Root, reader: &Reader) -> How {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        How::Whole
    } else {
        listed(root, reader)
    }
}

/// [`How::Folders`] as the reader lists them now, or [`How::Unread`].
fn listed(root: &Root, reader: &Reader) -> How {
    root.folders(reader).map_or(How::Unread, How::Folders)
}

/// One branch a window listens to.
#[derive(Clone)]
struct Listened {
    branch: WatchedBranch,
    root: Root,
    how: How,
}

struct Inner<W: notify::Watcher> {
    watcher: Option<W>,
    by_window: HashMap<String, Vec<Listened>>,
    newest: HashMap<String, u64>,
    watched: HashMap<PathBuf, RecursiveMode>,
    /// When each branch's folders were last listed again, by its folder, and whether a listing
    /// is already waiting for its turn: at most one a second (`RELISTED_EVERY`).
    relisted: HashMap<PathBuf, (Instant, bool)>,
    /// The most folders watched one by one, app-wide (`inotify_budget`).
    budget: usize,
    /// Each branch's check of what moved, by its folder: what is waiting to be asked, whether a
    /// folder appeared among it, and whether a worker is asking now.
    checking: HashMap<PathBuf, Check>,
    /// The clones listened to for their shared standing (FD-11), by the folder the standing
    /// names them by.
    kept: HashMap<PathBuf, Kept>,
    /// The clones being found, by that folder: asked once however often the standing asks.
    finding: HashSet<PathBuf>,
    /// The clones whose whole tree is watched now, which the standing has been told it covers.
    covering: HashSet<PathBuf>,
    /// How many times each plane has been let go of: a clone found for a plane let go of
    /// while it was being found is not kept.
    let_go: HashMap<PathBuf, u64>,
    /// Clones the reader would not find, and until when they are not asked for again.
    refused: HashMap<PathBuf, Instant>,
}

/// How long a clone the reader would not find is left alone before it is asked for again: the
/// standing asks on every read, and each ask would start a reader.
const REFUSED_FOR: Duration = Duration::from_secs(60);

impl<W: notify::Watcher> Inner<W> {
    /// Every branch and clone listened to, with how: the windows' and the standing's.
    fn listened(&self) -> impl Iterator<Item = (&Root, &How)> {
        self.by_window
            .values()
            .flatten()
            .map(|one| (&one.root, &one.how))
            .chain(self.kept.values().map(|kept| (&kept.root, &kept.how)))
    }
}

/// A clone listened to for its shared standing.
#[derive(Clone)]
struct Kept {
    plane: PathBuf,
    root: Root,
    how: How,
}

/// One branch's check of what moved in it.
#[derive(Default)]
struct Check {
    moved: Vec<PathBuf>,
    made_folder: bool,
    running: bool,
}

/// How often a branch's folders are listed again as folders appear in it, at most: an agent
/// making folders without pause lists them once a second, not on every burst.
const RELISTED_EVERY: Duration = Duration::from_secs(1);

/// How many folders the app watches one by one, at most, across every window and branch.
///
/// On inotify each one is a watch from the user's whole budget
/// (`/proc/sys/fs/inotify/max_user_watches`, 8,192 on older kernels), which the operator's
/// editor and every other watcher share: charter takes a quarter of it. Elsewhere a branch is
/// watched whole, and this does not bind.
fn inotify_budget() -> usize {
    if cfg!(target_os = "linux") {
        std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
            .ok()
            .and_then(|text| text.trim().parse::<usize>().ok())
            .map_or(2048, |max| (max / 4).max(256))
    } else {
        usize::MAX
    }
}

/// Every window's listened branches. Managed by the app; dropping it stops every watch.
pub struct BranchWatch<W: notify::Watcher = notify::RecommendedWatcher> {
    inner: Arc<Mutex<Inner<W>>>,
    told: Told,
    reader: Reader,
    config: notify::Config,
    tickets: std::sync::atomic::AtomicU64,
    /// How a clone kept for its standing is listened to: [`how`], the platform's way.
    how_of: fn(&Root, &Reader) -> How,
}

impl BranchWatch {
    /// A watch that tells `told`, on the platform's own watcher.
    pub fn new(told: Told, reader: Reader) -> Self {
        Self::with_config(told, reader, notify::Config::default())
    }
}

impl<W: notify::Watcher + Send + 'static> BranchWatch<W> {
    fn with_config(told: Told, reader: Reader, config: notify::Config) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                watcher: None,
                by_window: HashMap::new(),
                newest: HashMap::new(),
                watched: HashMap::new(),
                relisted: HashMap::new(),
                budget: inotify_budget(),
                checking: HashMap::new(),
                kept: HashMap::new(),
                finding: HashSet::new(),
                covering: HashSet::new(),
                let_go: HashMap::new(),
                refused: HashMap::new(),
            })),
            told,
            reader,
            config,
            tickets: std::sync::atomic::AtomicU64::new(0),
            how_of: how,
        }
    }

    /// The same watch, listening to the clones kept for their standing `how_of`'s way: for a
    /// test to try either way on any platform.
    #[cfg(test)]
    fn keeping_clones(mut self, how_of: fn(&Root, &Reader) -> How) -> Self {
        self.how_of = how_of;
        self
    }

    /// A number for a set a window asks for, larger than every one before it.
    pub fn ticket(&self) -> u64 {
        self.tickets
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(1)
    }

    /// `window` now listens to `branches`, each with its resolved folder and how; unless it
    /// has since asked for a newer set.
    pub fn set_from(
        &self,
        window: &str,
        ticket: u64,
        branches: Vec<(WatchedBranch, Root, How)>,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let newest = inner.newest.entry(window.to_string()).or_default();
        if ticket < *newest {
            return Ok(());
        }
        *newest = ticket;
        let listened: Vec<Listened> = branches
            .into_iter()
            .map(|(branch, root, how)| Listened { branch, root, how })
            .collect();
        if listened.is_empty() {
            inner.by_window.remove(window);
        } else {
            inner.by_window.insert(window.to_string(), listened);
        }
        if inner.watcher.is_none() && !inner.by_window.is_empty() {
            inner.watcher = Some(self.start()?);
        }
        inner.follow();
        Ok(())
    }

    /// `window` is gone: none of its branches is listened to for it any more.
    pub fn forget(&self, window: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.newest.remove(window);
        if inner.by_window.remove(window).is_some() {
            inner.follow();
        }
    }

    /// Listens to the clone `wanted` names for its shared standing (FD-11), and covers the
    /// standing once its whole tree is watched. Answers at once: the clone is found by the
    /// bounded reader on a thread of its own, once however often it is asked for.
    pub fn want(&self, wanted: &Wanted) {
        let let_go = {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if inner
                .refused
                .get(&wanted.path)
                .is_some_and(|until| Instant::now() < *until)
            {
                return;
            }
            if inner.kept.contains_key(&wanted.path) || !inner.finding.insert(wanted.path.clone()) {
                return;
            }
            inner.let_go.get(&wanted.plane).copied().unwrap_or_default()
        };
        let handle = Arc::downgrade(&self.inner);
        let reader = self.reader.clone();
        let told = Arc::clone(&self.told);
        let config = self.config;
        let how_of = self.how_of;
        let wanted = wanted.clone();
        let _ = std::thread::Builder::new()
            .name("charter-standing-watch".into())
            .spawn(move || {
                let found = purlis_core::files::root(
                    &reader,
                    &wanted.plane,
                    Branch::repo(&wanted.workspace, &wanted.repo),
                )
                .map(|root| {
                    let how = how_of(&root, &reader);
                    (root, how)
                });
                let Some(inner) = handle.upgrade() else {
                    return;
                };
                let mut held = inner.lock().unwrap_or_else(PoisonError::into_inner);
                held.finding.remove(&wanted.path);
                // Its plane was let go of while it was being found.
                if held.let_go.get(&wanted.plane).copied().unwrap_or_default() != let_go {
                    return;
                }
                // A clone the reader will not find is not watched, and not asked for again for a
                // while; the standing keeps its clock.
                let Ok((root, how)) = found else {
                    held.refused
                        .insert(wanted.path.clone(), Instant::now() + REFUSED_FOR);
                    return;
                };
                held.kept.insert(
                    wanted.path.clone(),
                    Kept {
                        plane: wanted.plane.clone(),
                        root,
                        how,
                    },
                );
                if held.watcher.is_none() {
                    match start::<W>(&inner, &reader, &told, config) {
                        Ok(watcher) => held.watcher = Some(watcher),
                        Err(_) => {
                            held.kept.remove(&wanted.path);
                            return;
                        }
                    }
                }
                held.follow();
            });
    }

    /// The plane at `plane` is let go of: none of its clones is listened to for its standing any
    /// more, and none is covered.
    pub fn let_go_of_plane(&self, plane: &std::path::Path) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        *inner.let_go.entry(plane.to_path_buf()).or_default() += 1;
        inner.refused.retain(|at, _| !at.starts_with(plane));
        let before = inner.kept.len();
        inner.kept.retain(|_, kept| kept.plane != plane);
        if inner.kept.len() != before {
            inner.follow();
        }
    }

    /// What is watched now, for a test to see a folder added as it appears: the poller the
    /// tests run on macOS sees a write in a new folder from its parent's listing, so hearing
    /// one there would not show the new folder is watched.
    #[cfg(test)]
    fn watching(&self) -> Vec<PathBuf> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.watched.keys().cloned().collect()
    }

    /// The reader this watch asks: for a branch's folders when it is resolved.
    pub fn reader(&self) -> &Reader {
        &self.reader
    }

    /// The platform's watcher, and a thread that folds what it reports into bursts and hands
    /// each branch that something moved in to its worker. The thread ends when the watcher is
    /// dropped, which drops the sending half it reads.
    fn start(&self) -> Result<W, String> {
        start::<W>(&self.inner, &self.reader, &self.told, self.config)
    }
}

/// [`BranchWatch::start`], for a caller that holds only the watch's insides.
fn start<W: notify::Watcher + Send + 'static>(
    inner: &Arc<Mutex<Inner<W>>>,
    reader: &Reader,
    told: &Told,
    config: notify::Config,
) -> Result<W, String> {
    let (sent, events) = mpsc::channel();
    let handle: Weak<Mutex<Inner<W>>> = Arc::downgrade(inner);
    let told = Arc::clone(told);
    let reader = reader.clone();
    std::thread::Builder::new()
        .name("charter-branch-watch".into())
        .spawn(move || {
            for burst in crate::watchset::bursts(events, QUIET_FOR, MOST_PATHS) {
                let Some(inner) = handle.upgrade() else {
                    return;
                };
                heard(&inner, &reader, &told, &burst);
            }
        })
        .map_err(|e| format!("purlis could not watch the branch: {e}"))?;
    // No file-id cache, so nothing walks the tree under a watch — a link in the branch to the
    // operator's home is never followed and read (R2) — and links are not followed.
    W::new(
        crate::watchset::sender(sent),
        config.with_follow_symlinks(false),
    )
    .map_err(|e| format!("purlis could not watch the branch: {e}"))
}

/// Hands each branch that `burst` moved something in to its worker. Nothing is read here: this
/// thread goes straight back to the platform's events.
fn heard<W: notify::Watcher + Send + 'static>(
    inner: &Arc<Mutex<Inner<W>>>,
    reader: &Reader,
    told: &Told,
    burst: &crate::watchset::Burst,
) {
    let roots: Vec<Root> = {
        let mut held = inner.lock().unwrap_or_else(PoisonError::into_inner);
        if burst.lost {
            held.watch_again();
        }
        let mut seen: HashSet<PathBuf> = HashSet::new();
        held.listened()
            .map(|(root, _)| root)
            .filter(|root| seen.insert(root.path().to_path_buf()))
            .cloned()
            .collect()
    };
    for root in roots {
        // Everything moved the branch's folder itself, which always matters (`Root::matters`).
        let inside: Vec<PathBuf> = if burst.everything {
            vec![root.path().to_path_buf()]
        } else {
            burst
                .paths
                .iter()
                .filter(|path| path.starts_with(root.path()))
                .cloned()
                .collect()
        };
        if !inside.is_empty() {
            check(inner, reader, told, root, inside, burst.made_folder);
        }
    }
}

/// Hands what moved in `root` to its branch's worker: started now when none is asking, else
/// asked next. One worker per branch at a time, so a hung check holds one thread, not one per
/// burst.
fn check<W: notify::Watcher + Send + 'static>(
    inner: &Arc<Mutex<Inner<W>>>,
    reader: &Reader,
    told: &Told,
    root: Root,
    moved: Vec<PathBuf>,
    made_folder: bool,
) {
    {
        let mut held = inner.lock().unwrap_or_else(PoisonError::into_inner);
        let waiting = held.checking.entry(root.path().to_path_buf()).or_default();
        // Past what one check looks at, the rest only says "more": the burst matters anyway.
        let room = (purlis_core::files::ASKED + 1).saturating_sub(waiting.moved.len());
        waiting.moved.extend(moved.into_iter().take(room));
        waiting.made_folder |= made_folder;
        if waiting.running {
            return;
        }
        waiting.running = true;
    }
    let handle = Arc::downgrade(inner);
    let reader = reader.clone();
    let told = Arc::clone(told);
    std::thread::spawn(move || {
        loop {
            let Some(inner) = handle.upgrade() else {
                return;
            };
            let (moved, made_folder) = {
                let mut held = inner.lock().unwrap_or_else(PoisonError::into_inner);
                let Some(waiting) = held.checking.get_mut(root.path()) else {
                    return;
                };
                if waiting.moved.is_empty() {
                    held.checking.remove(root.path());
                    return;
                }
                (
                    std::mem::take(&mut waiting.moved),
                    std::mem::take(&mut waiting.made_folder),
                )
            };
            // Outside the lock: this is the read that can take until the reader's deadline.
            if !root.matters(&reader, &moved) {
                continue;
            }
            // The clone's shared standing reads git again at its next ask (FD-11), whoever
            // listens: the explorer's branch of a clone is the clone the Saving row reads.
            let standings: Vec<PathBuf> = {
                let held = inner.lock().unwrap_or_else(PoisonError::into_inner);
                held.kept
                    .iter()
                    .filter(|(_, kept)| kept.root.path() == root.path())
                    .map(|(at, _)| at.clone())
                    .collect()
            };
            purlis_core::planegit::touch(root.path());
            for at in standings {
                purlis_core::planegit::touch(&at);
            }
            if made_folder {
                relist_soon(&inner, &reader, root.clone());
            }
            let concerned: Vec<(String, WatchedBranch)> = {
                let held = inner.lock().unwrap_or_else(PoisonError::into_inner);
                held.by_window
                    .iter()
                    .flat_map(|(window, listened)| {
                        listened
                            .iter()
                            .filter(|one| one.root.path() == root.path())
                            .map(|one| (window.clone(), one.branch.clone()))
                    })
                    .collect()
            };
            let mut by_window: HashMap<String, Vec<WatchedBranch>> = HashMap::new();
            for (window, branch) in concerned {
                let told = by_window.entry(window).or_default();
                if !told.contains(&branch) {
                    told.push(branch);
                }
            }
            for (window, branches) in by_window {
                told(&window, branches);
            }
        }
    });
}

/// Lists `root`'s folders again, now or — when it was listed less than [`RELISTED_EVERY`] ago —
/// once that has passed, on a thread of its own; never twice in that time. Only for a branch
/// watched folder by folder.
fn relist_soon<W: notify::Watcher + Send + 'static>(
    inner: &Arc<Mutex<Inner<W>>>,
    reader: &Reader,
    root: Root,
) {
    let wait = {
        let mut held = inner.lock().unwrap_or_else(PoisonError::into_inner);
        let one_by_one = held.listened().any(|(one, how)| {
            one.path() == root.path() && matches!(how, How::Folders(_) | How::Unread)
        });
        if !one_by_one {
            return;
        }
        let now = Instant::now();
        let at = root.path().to_path_buf();
        match held.relisted.get(&at).copied() {
            Some((_, true)) => return,
            Some((last, false)) if now.duration_since(last) < RELISTED_EVERY => {
                held.relisted.insert(at, (last, true));
                Some(RELISTED_EVERY - now.duration_since(last))
            }
            _ => {
                held.relisted.insert(at, (now, false));
                None
            }
        }
    };
    let handle = Arc::downgrade(inner);
    let reader = reader.clone();
    let run = move || {
        let now = listed(&root, &reader);
        let Some(inner) = handle.upgrade() else {
            return;
        };
        let mut held = inner.lock().unwrap_or_else(PoisonError::into_inner);
        held.relisted
            .insert(root.path().to_path_buf(), (Instant::now(), false));
        let held = &mut *held;
        let hows = held
            .by_window
            .values_mut()
            .flatten()
            .map(|one| (&one.root, &mut one.how))
            .chain(
                held.kept
                    .values_mut()
                    .map(|kept| (&kept.root, &mut kept.how)),
            );
        for (one, how) in hows {
            if one.path() == root.path() && matches!(how, How::Folders(_) | How::Unread) {
                *how = now.clone();
            }
        }
        held.follow();
    };
    match wait {
        None => run(),
        Some(wait) => {
            std::thread::spawn(move || {
                std::thread::sleep(wait);
                run();
            });
        }
    }
}

impl<W: notify::Watcher> Inner<W> {
    /// Watches what every window's branches need, and stops watching what none needs.
    fn follow(&mut self) {
        let mut wanted: HashMap<PathBuf, RecursiveMode> = HashMap::new();
        let mut one_by_one: Vec<(usize, PathBuf)> = Vec::new();
        for (root, how) in self.listened() {
            match how {
                How::Whole => {
                    wanted.insert(root.path().to_path_buf(), RecursiveMode::Recursive);
                }
                How::Unread => one_by_one.push((0, root.path().to_path_buf())),
                How::Folders(folders) => {
                    for folder in folders {
                        let depth = folder
                            .strip_prefix(root.path())
                            .map_or(usize::MAX, |below| below.components().count());
                        one_by_one.push((depth, folder.clone()));
                    }
                }
            }
        }
        // Within the app's share of the platform's watches, the shallowest first: past it, a
        // deep folder is heard only when something above it moves.
        one_by_one.sort();
        one_by_one.dedup_by(|a, b| a.1 == b.1);
        for (_, folder) in one_by_one.into_iter().take(self.budget) {
            wanted.entry(folder).or_insert(RecursiveMode::NonRecursive);
        }
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let stale: Vec<PathBuf> = self
            .watched
            .iter()
            .filter(|(path, mode)| wanted.get(*path) != Some(*mode))
            .map(|(path, _)| path.clone())
            .collect();
        for path in stale {
            let _ = watcher.unwatch(&path);
            self.watched.remove(&path);
        }
        for (path, mode) in wanted {
            if self.watched.contains_key(&path) {
                continue;
            }
            match watcher.watch(&path, mode) {
                Ok(()) => {
                    self.watched.insert(path, mode);
                }
                // The platform's watches are spent: what is watched already keeps working,
                // and nothing more is asked of it.
                Err(e) if matches!(e.kind, notify::ErrorKind::MaxFilesWatch) => break,
                Err(_) => {}
            }
        }
        self.cover();
    }

    /// Tells the shared standing which kept clones are watched whole now (FD-11): only those
    /// a recursive watch holds (D-FD11h). A clone that starts or stops being covered is read
    /// again at its next ask.
    fn cover(&mut self) {
        let whole = |kept: &Kept| {
            kept.how == How::Whole
                && self.watched.get(kept.root.path()) == Some(&RecursiveMode::Recursive)
        };
        let now: HashSet<PathBuf> = self
            .kept
            .iter()
            .filter(|(_, kept)| whole(kept))
            .map(|(at, _)| at.clone())
            .collect();
        for gone in self.covering.difference(&now) {
            purlis_core::reposave::cover(gone, false);
        }
        for new in now.difference(&self.covering) {
            purlis_core::reposave::cover(new, true);
        }
        self.covering = now;
    }

    /// The platform lost track, or its watcher erred: what it said about the watched trees
    /// may be incomplete. Every kept clone is uncovered and its tree watched again, and only a
    /// watch made now covers it once more (D-FD11h).
    fn watch_again(&mut self) {
        let whole: Vec<PathBuf> = self
            .kept
            .values()
            .filter(|kept| kept.how == How::Whole)
            .map(|kept| kept.root.path().to_path_buf())
            .collect();
        for gone in self.covering.drain() {
            purlis_core::reposave::cover(&gone, false);
        }
        if let Some(watcher) = self.watcher.as_mut() {
            for root in whole {
                if self.watched.remove(&root).is_some() {
                    let _ = watcher.unwatch(&root);
                }
            }
        }
        self.follow();
    }
}

/// Listens to every branch named, so the window hears when anything in one moves that its
/// changes could show (FM-4). A branch that does not resolve is not listened to. The set
/// replaces the window's last one.
// Each branch is resolved by `purlis_core::files::root`, with the tree's own checks; the window
// never names a directory. Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn branch_watch(
    window: tauri::Window,
    planes: tauri::State<'_, Planes>,
    branches: Vec<WatchedBranch>,
) -> Result<(), String> {
    use tauri::Manager as _;
    let ticket = window.state::<BranchWatch>().ticket();
    let reader = window.state::<BranchWatch>().reader().clone();
    let mut asked: Vec<(PathBuf, WatchedBranch)> = Vec::new();
    let mut seen: HashSet<WatchedBranch> = HashSet::new();
    for branch in branches {
        if asked.len() >= BRANCHES || !seen.insert(branch.clone()) {
            continue;
        }
        let Ok(held) = planes.held(&branch.plane) else {
            continue;
        };
        asked.push((held.root().to_path_buf(), branch));
    }
    // Off the thread that draws (SC-2): finding a branch, and on inotify its folders, asks the
    // bounded reader.
    let resolved = tauri::async_runtime::spawn_blocking(move || resolve(&reader, asked))
        .await
        .map_err(|err| format!("watching the branch did not finish: {err}"))?;
    window
        .state::<BranchWatch>()
        .set_from(window.label(), ticket, resolved)
}

/// Each branch's folder and how it is listened to; a branch that does not resolve is left out.
///
/// Each branch is asked on its own (#1130), so one whose read hangs until its deadline does not
/// hold back watching the window's others. [`BRANCHES`] bounds the threads, and the reader's own
/// app-wide gate how many children run at once.
fn resolve(
    reader: &Reader,
    asked: Vec<(PathBuf, WatchedBranch)>,
) -> Vec<(WatchedBranch, Root, How)> {
    each_apart(asked, |(plane, branch)| {
        let named = crate::piecefiles::branch(&branch.workspace, &branch.repo, &branch.piece);
        let root = purlis_core::files::root(reader, &plane, named).ok()?;
        let how = how(&root, reader);
        Some((branch, root, how))
    })
}

/// `each` of `asked`, every one on a thread of its own, answered in `asked`'s order; one that
/// answers nothing, or whose thread panicked, is left out.
fn each_apart<T: Send, R: Send>(asked: Vec<T>, each: impl Fn(T) -> Option<R> + Sync) -> Vec<R> {
    let each = &each;
    std::thread::scope(|scope| {
        let running: Vec<_> = asked
            .into_iter()
            .map(|one| scope.spawn(move || each(one)))
            .collect();
        running
            .into_iter()
            .filter_map(|thread| thread.join().ok().flatten())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::mpsc;

    const PATIENCE: Duration = Duration::from_secs(10);

    #[test]
    fn branches_are_resolved_apart_and_answered_in_their_order() {
        let started = Instant::now();
        let answered = each_apart(vec![300u64, 0, 300, 7, 300], |ms| {
            std::thread::sleep(Duration::from_millis(ms));
            // One that does not resolve is left out.
            (ms != 7).then_some(ms)
        });
        assert_eq!(answered, [300, 0, 300, 300]);
        // In turn they would take 900 ms; apart, about one wait.
        let took = started.elapsed();
        assert!(took < Duration::from_millis(800), "{took:?}");
    }

    #[cfg(target_os = "macos")]
    type Source = notify::PollWatcher;
    #[cfg(not(target_os = "macos"))]
    type Source = notify::RecommendedWatcher;

    fn git(dir: &Path, args: &[&str]) {
        let ran = purlis_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "user.email=t@e.invalid", "-c", "user.name=t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args),
        )
        .unwrap();
        assert!(ran.status.success(), "git {args:?}: {ran:?}");
    }

    /// A plane with a clone holding `src/` and an ignored `target/`, and one piece cut from it.
    fn plane() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let clone = root.join("workspaces/alpha/thing");
        std::fs::create_dir_all(clone.join("src/deep")).unwrap();
        git(&clone, &["init", "-q", "-b", "main", "."]);
        std::fs::write(clone.join("src/deep/lib.rs"), "\n").unwrap();
        std::fs::write(clone.join(".gitignore"), "target/\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "one"]);
        let piece = purlis_core::worktree::add(&root, "alpha", "thing", "piece", None)
            .unwrap()
            .path;
        std::fs::create_dir_all(piece.join("target/debug")).unwrap();
        (dir, root, piece)
    }

    fn branch() -> WatchedBranch {
        named("piece")
    }

    fn named(piece: &str) -> WatchedBranch {
        WatchedBranch {
            plane: serde_json::from_value(serde_json::json!("/plane")).unwrap(),
            workspace: "alpha".to_string(),
            repo: "thing".to_string(),
            piece: Some(piece.to_string()),
        }
    }

    fn root_of(plane: &Path, piece: &str) -> Root {
        purlis_core::files::root(
            &crate::reader(),
            plane,
            purlis_core::files::Branch::piece("alpha", "thing", piece),
        )
        .unwrap()
    }

    type Heard = mpsc::Receiver<(String, Vec<WatchedBranch>)>;

    fn watch_with(reader: Reader) -> (BranchWatch<Source>, Heard) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = BranchWatch::<Source>::with_config(
            Arc::new(move |window: &str, branches| {
                let _ = tx.lock().unwrap().send((window.to_string(), branches));
            }),
            reader,
            notify::Config::default().with_poll_interval(Duration::from_millis(50)),
        );
        (watch, rx)
    }

    fn listening(
        root: &Path,
        how: impl Fn(&Root) -> How,
    ) -> (
        BranchWatch<Source>,
        mpsc::Receiver<(String, Vec<WatchedBranch>)>,
    ) {
        let (watch, rx) = watch_with(crate::reader());
        let found = root_of(root, "piece");
        let chosen = how(&found);
        watch
            .set_from("main", watch.ticket(), vec![(branch(), found, chosen)])
            .unwrap();
        // The poller's first look is its baseline; give it one.
        std::thread::sleep(Duration::from_millis(300));
        (watch, rx)
    }

    fn both_ways() -> [fn(&Root) -> How; 2] {
        [
            |_| How::Whole,
            |root| How::Folders(root.folders(&crate::reader()).unwrap()),
        ]
    }

    #[test]
    fn a_first_write_deep_in_a_folder_nobody_opened_tells_the_window() {
        for how in both_ways() {
            let (_dir, root, piece) = plane();
            let (_watch, told) = listening(&root, how);

            std::fs::write(piece.join("src/deep/first.rs"), "\n").unwrap();

            let (window, branches) = told.recv_timeout(PATIENCE).expect("the window was told");
            assert_eq!(window, "main");
            assert_eq!(branches, [branch()]);
        }
    }

    #[test]
    fn a_write_only_where_git_ignores_tells_nobody() {
        for how in both_ways() {
            let (_dir, root, piece) = plane();
            let (_watch, told) = listening(&root, how);

            std::fs::write(piece.join("target/debug/out"), "a build's\n").unwrap();

            assert!(told.recv_timeout(Duration::from_secs(2)).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_the_branch_to_a_large_tree_outside_is_never_walked() {
        let (_dir, root, piece) = plane();
        let outside = tempfile::tempdir().unwrap();
        for folder in 0..300 {
            let at = outside.path().join(format!("f{folder}"));
            std::fs::create_dir_all(&at).unwrap();
            for file in 0..300 {
                std::fs::write(at.join(format!("{file}.txt")), "").unwrap();
            }
        }
        std::os::unix::fs::symlink(outside.path(), piece.join("home")).unwrap();
        // The same branch with nothing behind it: the baseline, under whatever load this runs.
        purlis_core::worktree::add(&root, "alpha", "thing", "plain", None).unwrap();
        // The platform's own watcher, as the app runs it, on the whole branch; each time a fresh
        // one, so nothing one watch learned helps the next.
        let watching = |piece: &str| {
            let watch = BranchWatch::<notify::RecommendedWatcher>::with_config(
                Arc::new(|_: &str, _| {}),
                crate::reader(),
                notify::Config::default(),
            );
            let found = root_of(&root, piece);
            let started = std::time::Instant::now();
            watch
                .set_from(
                    "main",
                    watch.ticket(),
                    vec![(named(piece), found, How::Whole)],
                )
                .unwrap();
            started.elapsed()
        };

        // Interleaved, the fastest of three each: a moment of load slows one sample, not the
        // comparison.
        let (mut linked, mut plain) = (Duration::MAX, Duration::MAX);
        for _ in 0..3 {
            linked = linked.min(watching("piece"));
            plain = plain.min(watching("plain"));
        }

        // Measured on macOS: with a debouncer's file-id cache the 90,000 files behind the link
        // were walked, about a second against 25 ms without it. Without a walk the link costs
        // nothing; the bound is the baseline's, loosely.
        let bound = (plain * 4).max(plain + Duration::from_millis(250));
        assert!(
            linked < bound,
            "watching the linked branch took {linked:?}, the plain one {plain:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_branch_whose_check_hangs_holds_up_no_other_branch() {
        let (_dir, root, piece) = plane();
        let other = purlis_core::worktree::add(&root, "alpha", "thing", "other", None)
            .unwrap()
            .path;
        // An ignore file that blocks whoever opens it: the check of this branch hangs.
        let made = purlis_core::forklock::output(
            std::process::Command::new("mkfifo").arg(piece.join("src/.gitignore")),
        )
        .unwrap();
        assert!(made.status.success());
        let deadline = Duration::from_secs(6);
        let (watch, told) = watch_with(crate::reader().deadline(deadline));
        let listened = ["piece", "other"]
            .map(|one| (named(one), root_of(&root, one), How::Whole))
            .to_vec();
        watch.set_from("main", watch.ticket(), listened).unwrap();
        std::thread::sleep(Duration::from_millis(300));

        let started = std::time::Instant::now();
        std::fs::write(piece.join("src/deep/hangs.rs"), "\n").unwrap();
        std::fs::write(other.join("src/deep/moves.rs"), "\n").unwrap();

        // The other branch is told while the first one's check still hangs.
        let (_, first) = told.recv_timeout(PATIENCE).expect("a window was told");
        assert_eq!(first, [named("other")]);
        assert!(started.elapsed() < deadline, "{:?}", started.elapsed());
        // And the hung one, once the reader gave up on it: a check that did not answer matters.
        let (_, then) = told
            .recv_timeout(PATIENCE)
            .expect("the hung branch was told");
        assert_eq!(then, [named("piece")]);
    }

    /// A watch on [`crate::watchset::raw::Raw`], listening to the piece whole: the test plays
    /// the platform.
    fn listening_raw(
        root: &Path,
    ) -> (
        BranchWatch<crate::watchset::raw::Raw>,
        mpsc::Receiver<(String, Vec<WatchedBranch>)>,
    ) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = BranchWatch::<crate::watchset::raw::Raw>::with_config(
            Arc::new(move |window: &str, branches| {
                let _ = tx.lock().unwrap().send((window.to_string(), branches));
            }),
            crate::reader(),
            notify::Config::default(),
        );
        watch
            .set_from(
                "main",
                watch.ticket(),
                vec![(branch(), root_of(root, "piece"), How::Whole)],
            )
            .unwrap();
        (watch, rx)
    }

    #[test]
    fn a_platform_that_lost_track_tells_every_branch() {
        // inotify's queue overflowed, or the watcher erred: anything may have moved, so the
        // branch's markers are read again rather than left as they were (#1139).
        let (_dir, root, _piece) = plane();
        let (_watch, told) = listening_raw(&root);

        crate::watchset::raw::event(
            notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan),
        );

        let (window, branches) = told.recv_timeout(PATIENCE).expect("the window was told");
        assert_eq!(window, "main");
        assert_eq!(branches, [branch()]);
    }

    #[test]
    fn a_file_made_and_removed_inside_one_burst_is_still_a_move() {
        // A debouncer took the removal as cancelling the creation and told neither (#1139).
        use notify::event::{CreateKind, EventKind, RemoveKind};
        let (_dir, root, piece) = plane();
        let (_watch, told) = listening_raw(&root);
        let brief = std::fs::canonicalize(&piece)
            .unwrap()
            .join("src/deep/brief.rs");

        crate::watchset::raw::raw(EventKind::Create(CreateKind::File), &brief);
        crate::watchset::raw::raw(EventKind::Remove(RemoveKind::File), &brief);

        let (_, branches) = told.recv_timeout(PATIENCE).expect("the window was told");
        assert_eq!(branches, [branch()]);
    }

    /// What the shared standing asks of the watch for the plane's clone (FD-11).
    fn the_clone(root: &Path) -> purlis_core::standings::Wanted {
        purlis_core::standings::Wanted {
            plane: root.to_path_buf(),
            workspace: "alpha".into(),
            repo: "thing".into(),
            path: root.join("workspaces/alpha/thing"),
        }
    }

    fn standing_of(wanted: &purlis_core::standings::Wanted) -> purlis_core::reposave::Standing {
        purlis_core::reposave::shared_standing(
            &wanted.plane,
            &wanted.workspace,
            &purlis_core::repos::Repo {
                name: wanted.repo.clone(),
                path: wanted.path.clone(),
            },
        )
    }

    /// Waits, at most [`PATIENCE`], for `done`.
    fn until(done: impl Fn() -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < PATIENCE {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        done()
    }

    #[test]
    fn a_clone_whose_standing_is_shared_is_read_again_when_its_tree_moves_not_on_a_clock() {
        // FD-11 (#651): the one watch per repo covers the clone, so its standing is read again
        // when something git would show moves in it, and otherwise only on its long backstop.
        let (_dir, root, _piece) = plane();
        let (watch, _told) = watch_with(crate::reader());
        let watch = watch.keeping_clones(|_, _| How::Whole);
        let clone = the_clone(&root);

        watch.want(&clone);

        assert!(until(|| purlis_core::reposave::covered(&clone.path)));
        // The poller's first look is its baseline.
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(standing_of(&clone).changed, 0);
        let started = Instant::now();
        std::fs::write(clone.path.join("src/deep/first.rs"), "\n").unwrap();
        assert!(until(|| standing_of(&clone).changed == 1));
        // Well inside the backstop a clone nothing watches is read again on.
        assert!(
            started.elapsed() < purlis_core::planegit::SHARED_FOR,
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_plane_let_go_of_no_longer_covers_its_clones() {
        let (_dir, root, _piece) = plane();
        let (watch, _told) = watch_with(crate::reader());
        let watch = watch.keeping_clones(|_, _| How::Whole);
        let clone = the_clone(&root);
        watch.want(&clone);
        assert!(until(|| purlis_core::reposave::covered(&clone.path)));

        watch.let_go_of_plane(&root);

        assert!(!purlis_core::reposave::covered(&clone.path));
        assert!(watch.watching().is_empty(), "{:?}", watch.watching());
    }

    #[test]
    fn a_clone_watched_folder_by_folder_is_never_covered() {
        // D-FD11h: inotify's watches are one per folder, and a folder made, or deleted and
        // made again, between a listing and its watch is an edit nobody hears; the standing
        // keeps its clock there.
        for how_of in [
            (|root: &Root, reader: &Reader| How::Folders(root.folders(reader).unwrap()))
                as fn(&Root, &Reader) -> How,
            |_, _| How::Unread,
        ] {
            let (_dir, root, _piece) = plane();
            let (watch, _told) = watch_with(crate::reader());
            let watch = watch.keeping_clones(how_of);
            let clone = the_clone(&root);
            let at = root_of_clone(&root);

            watch.want(&clone);

            assert!(until(|| watch
                .watching()
                .contains(&at.path().to_path_buf())));
            std::thread::sleep(Duration::from_millis(200));
            assert!(!purlis_core::reposave::covered(&clone.path));
        }
    }

    #[test]
    fn a_lost_watch_uncovers_the_clone_until_its_tree_is_watched_again() {
        let (_dir, root, _piece) = plane();
        let (tx, _rx) = mpsc::channel::<(String, Vec<WatchedBranch>)>();
        let tx = Mutex::new(tx);
        let watch = BranchWatch::<crate::watchset::raw::Raw>::with_config(
            Arc::new(move |window: &str, branches| {
                let _ = tx.lock().unwrap().send((window.to_string(), branches));
            }),
            crate::reader(),
            notify::Config::default(),
        )
        .keeping_clones(|_, _| How::Whole);
        // The platform's watcher, made on this thread, which plays it.
        watch
            .set_from(
                "main",
                watch.ticket(),
                vec![(branch(), root_of(&root, "piece"), How::Whole)],
            )
            .unwrap();
        let clone = the_clone(&root);
        let at = root_of_clone(&root).path().to_path_buf();
        watch.want(&clone);
        assert!(until(|| purlis_core::reposave::covered(&clone.path)));
        assert_eq!(crate::watchset::raw::watched(&at), 1);

        // The platform lost track: watched again, and covered once that watch is made.
        crate::watchset::raw::event(
            notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan),
        );
        assert!(until(|| crate::watchset::raw::watched(&at) == 2));
        assert!(purlis_core::reposave::covered(&clone.path));

        // Lost again, and this time the tree cannot be watched: it stays uncovered.
        crate::watchset::raw::refuse(&at);
        crate::watchset::raw::event(
            notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan),
        );
        assert!(until(|| !purlis_core::reposave::covered(&clone.path)));
        std::thread::sleep(Duration::from_millis(300));
        assert!(!purlis_core::reposave::covered(&clone.path));
    }

    /// The plane's clone, found as the standing's watch finds it.
    fn root_of_clone(plane: &Path) -> Root {
        purlis_core::files::root(
            &crate::reader(),
            plane,
            purlis_core::files::Branch::repo("alpha", "thing"),
        )
        .unwrap()
    }

    #[test]
    fn a_clone_the_reader_will_not_find_is_not_asked_for_again_at_once() {
        let (_dir, root, _piece) = plane();
        let (watch, _told) = watch_with(crate::reader());
        let mut missing = the_clone(&root);
        missing.repo = "nowhere".into();
        missing.path = root.join("workspaces/alpha/nowhere");

        watch.want(&missing);
        assert!(until(|| {
            let inner = watch.inner.lock().unwrap();
            inner.refused.contains_key(&missing.path) && inner.finding.is_empty()
        }));
        watch.want(&missing);

        assert!(
            watch.inner.lock().unwrap().finding.is_empty(),
            "asked again"
        );
    }

    #[test]
    fn a_clone_found_after_its_plane_was_let_go_of_is_not_kept() {
        let (_dir, root, _piece) = plane();
        let (watch, _told) = watch_with(crate::reader());
        let watch = watch.keeping_clones(|_, _| How::Whole);
        let clone = the_clone(&root);

        watch.want(&clone);
        watch.let_go_of_plane(&root);

        assert!(until(|| watch.inner.lock().unwrap().finding.is_empty()));
        assert!(watch.inner.lock().unwrap().kept.is_empty());
        assert!(!purlis_core::reposave::covered(&clone.path));
    }

    #[test]
    fn a_folder_made_in_the_branch_is_listened_in_once_it_appears() {
        let (_dir, root, piece) = plane();
        let (watch, told) = listening(&root, |root| {
            How::Folders(root.folders(&crate::reader()).unwrap())
        });
        let new = std::fs::canonicalize(&piece).unwrap().join("src/new");
        assert!(!watch.watching().contains(&new));

        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("a.rs"), "\n").unwrap();

        told.recv_timeout(PATIENCE)
            .expect("the new folder was heard");
        assert!(watch.watching().contains(&new), "{:?}", watch.watching());
    }
}
