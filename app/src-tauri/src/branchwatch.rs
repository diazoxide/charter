//! Tells a window when anything in a branch it shows the changes of moves (FM-4, #1107): an
//! agent's first write into a folder nobody opened is heard, and the branch's change markers
//! are read again without a refresh.
//!
//! **Not the tree's watch** (`filewatch.rs`), which watches exactly the folders a window has
//! open so it can read each again. This one listens to the whole branch, and says only *which
//! branch* moved: the window reads its status again.
//!
//! **What the whole branch costs depends on the platform**, so it is watched two ways
//! (`charter_core::files::Root`):
//! - where one watch covers a tree (FSEvents on macOS, `ReadDirectoryChangesW` on Windows), the
//!   branch's folder, recursively. Adding folders one at a time there restarts the stream each
//!   time, measured at about 6 ms a folder on macOS, so a branch of 2,000 folders would take 12
//!   seconds to watch; one recursive watch takes milliseconds.
//! - where a recursive watch is one per folder (inotify), the folders git knows, each
//!   non-recursively, so `node_modules/` and `target/` cost nothing. A folder created in one is
//!   added when it appears.
//!
//! **A move that cannot change the status is not told**: one only inside what git ignores (a
//! build writing `target/`), or inside git's own folder. Asked of git once per debounced batch.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use charter_core::files::Root;
use notify::RecursiveMode;
use notify::event::{CreateKind, EventKind};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer_opt};

use crate::planes::{PlaneId, Planes};

/// The event a window is sent.
pub(crate) const CHANGED: &str = "branch-changed";

/// How long a burst is folded for, as the tree's watch folds it.
const QUIET_FOR: Duration = Duration::from_millis(250);

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
}

/// The platform's way: [`How::Whole`] where one watch covers a tree.
pub fn how(root: &Root) -> How {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        How::Whole
    } else {
        How::Folders(
            root.folders()
                .unwrap_or_else(|_| vec![root.path().to_path_buf()]),
        )
    }
}

/// One branch a window listens to.
#[derive(Clone)]
struct Listened {
    branch: WatchedBranch,
    root: Root,
    how: How,
}

struct Inner<W: notify::Watcher> {
    debouncer: Option<Debouncer<W, RecommendedCache>>,
    by_window: HashMap<String, Vec<Listened>>,
    newest: HashMap<String, u64>,
    watched: HashMap<PathBuf, RecursiveMode>,
}

/// Every window's listened branches. Managed by the app; dropping it stops every watch.
pub struct BranchWatch<W: notify::Watcher = notify::RecommendedWatcher> {
    inner: Arc<Mutex<Inner<W>>>,
    told: Told,
    config: notify::Config,
    tickets: std::sync::atomic::AtomicU64,
}

impl BranchWatch {
    /// A watch that tells `told`, on the platform's own watcher.
    pub fn new(told: Told) -> Self {
        Self::with_config(told, notify::Config::default())
    }
}

impl<W: notify::Watcher + Send + 'static> BranchWatch<W> {
    fn with_config(told: Told, config: notify::Config) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                debouncer: None,
                by_window: HashMap::new(),
                newest: HashMap::new(),
                watched: HashMap::new(),
            })),
            told,
            config,
            tickets: std::sync::atomic::AtomicU64::new(0),
        }
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
        if inner.debouncer.is_none() && !inner.by_window.is_empty() {
            inner.debouncer = Some(self.start()?);
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

    /// What is watched now, for a test to see a folder added as it appears: the poller the
    /// tests run on macOS sees a write in a new folder from its parent's listing, so hearing
    /// one there would not show the new folder is watched.
    #[cfg(test)]
    fn watching(&self) -> Vec<PathBuf> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.watched.keys().cloned().collect()
    }

    fn start(&self) -> Result<Debouncer<W, RecommendedCache>, String> {
        let handle: Weak<Mutex<Inner<W>>> = Arc::downgrade(&self.inner);
        let told = Arc::clone(&self.told);
        let tell =
            move |batch: DebounceEventResult| {
                let Ok(events) = batch else { return };
                let mut moved: Vec<PathBuf> = Vec::new();
                let mut made_folder = false;
                for event in events
                    .iter()
                    .filter(|event| crate::watchset::matters(&event.kind))
                {
                    made_folder |= matches!(
                        event.kind,
                        EventKind::Create(CreateKind::Folder | CreateKind::Any)
                    );
                    moved.extend(event.paths.iter().cloned());
                }
                if moved.is_empty() {
                    return;
                }
                let Some(inner) = handle.upgrade() else {
                    return;
                };
                // Who listens where, copied out: asking git happens outside the lock.
                let listening: Vec<(String, Listened)> = {
                    let inner = inner.lock().unwrap_or_else(PoisonError::into_inner);
                    inner
                        .by_window
                        .iter()
                        .flat_map(|(window, listened)| {
                            listened.iter().map(|one| (window.clone(), one.clone()))
                        })
                        .collect()
                };
                let mut by_window: HashMap<String, Vec<WatchedBranch>> = HashMap::new();
                let mut grown: Vec<(String, WatchedBranch, How)> = Vec::new();
                for (window, one) in listening {
                    let inside: Vec<PathBuf> = moved
                        .iter()
                        .filter(|path| path.starts_with(one.root.path()))
                        .cloned()
                        .collect();
                    if inside.is_empty() || !one.root.matters(&inside) {
                        continue;
                    }
                    if made_folder && matches!(one.how, How::Folders(_)) {
                        let now = one
                            .root
                            .folders()
                            .unwrap_or_else(|_| vec![one.root.path().to_path_buf()]);
                        grown.push((window.clone(), one.branch.clone(), How::Folders(now)));
                    }
                    let told = by_window.entry(window).or_default();
                    if !told.contains(&one.branch) {
                        told.push(one.branch);
                    }
                }
                if !grown.is_empty() {
                    let mut inner = inner.lock().unwrap_or_else(PoisonError::into_inner);
                    for (window, branch, now) in grown {
                        if let Some(one) = inner.by_window.get_mut(&window).and_then(|listened| {
                            listened.iter_mut().find(|one| one.branch == branch)
                        }) {
                            one.how = now;
                        }
                    }
                    inner.follow();
                }
                for (window, branches) in by_window {
                    told(&window, branches);
                }
            };
        new_debouncer_opt(QUIET_FOR, None, tell, RecommendedCache::new(), self.config)
            .map_err(|e| format!("charter could not watch the branch: {e}"))
    }
}

impl<W: notify::Watcher> Inner<W> {
    /// Watches what every window's branches need, and stops watching what none needs.
    fn follow(&mut self) {
        let mut wanted: HashMap<PathBuf, RecursiveMode> = HashMap::new();
        for one in self.by_window.values().flatten() {
            match &one.how {
                How::Whole => {
                    wanted.insert(one.root.path().to_path_buf(), RecursiveMode::Recursive);
                }
                How::Folders(folders) => {
                    for folder in folders {
                        wanted
                            .entry(folder.clone())
                            .or_insert(RecursiveMode::NonRecursive);
                    }
                }
            }
        }
        let Some(debouncer) = self.debouncer.as_mut() else {
            return;
        };
        let stale: Vec<PathBuf> = self
            .watched
            .iter()
            .filter(|(path, mode)| wanted.get(*path) != Some(*mode))
            .map(|(path, _)| path.clone())
            .collect();
        for path in stale {
            let _ = debouncer.unwatch(&path);
            self.watched.remove(&path);
        }
        for (path, mode) in wanted {
            if self.watched.contains_key(&path) {
                continue;
            }
            if debouncer.watch(&path, mode).is_ok() {
                self.watched.insert(path, mode);
            }
        }
    }
}

/// Listens to every branch named, so the window hears when anything in one moves that its
/// changes could show (FM-4). A branch that does not resolve is not listened to. The set
/// replaces the window's last one.
// Each branch is resolved by `charter_core::files::root`, with the tree's own checks; the window
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
    // Off the thread that draws (SC-2): finding a branch, and on inotify its folders, asks git.
    let resolved = tauri::async_runtime::spawn_blocking(move || resolve(asked))
        .await
        .map_err(|err| format!("watching the branch did not finish: {err}"))?;
    window
        .state::<BranchWatch>()
        .set_from(window.label(), ticket, resolved)
}

/// Each branch's folder and how it is listened to; a branch that does not resolve is left out.
fn resolve(asked: Vec<(PathBuf, WatchedBranch)>) -> Vec<(WatchedBranch, Root, How)> {
    asked
        .into_iter()
        .filter_map(|(plane, branch)| {
            let named = crate::piecefiles::branch(&branch.workspace, &branch.repo, &branch.piece);
            let root = charter_core::files::root(&plane, named).ok()?;
            let how = how(&root);
            Some((branch, root, how))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::mpsc;

    const PATIENCE: Duration = Duration::from_secs(10);

    #[cfg(target_os = "macos")]
    type Source = notify::PollWatcher;
    #[cfg(not(target_os = "macos"))]
    type Source = notify::RecommendedWatcher;

    fn git(dir: &Path, args: &[&str]) {
        let ran = charter_core::forklock::output(
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
        let piece = charter_core::worktree::add(&root, "alpha", "thing", "piece", None)
            .unwrap()
            .path;
        std::fs::create_dir_all(piece.join("target/debug")).unwrap();
        (dir, root, piece)
    }

    fn branch() -> WatchedBranch {
        WatchedBranch {
            plane: serde_json::from_value(serde_json::json!("/plane")).unwrap(),
            workspace: "alpha".to_string(),
            repo: "thing".to_string(),
            piece: Some("piece".to_string()),
        }
    }

    fn listening(
        root: &Path,
        how: impl Fn(&Root) -> How,
    ) -> (
        BranchWatch<Source>,
        mpsc::Receiver<(String, Vec<WatchedBranch>)>,
    ) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = BranchWatch::<Source>::with_config(
            Arc::new(move |window: &str, branches| {
                let _ = tx.lock().unwrap().send((window.to_string(), branches));
            }),
            notify::Config::default().with_poll_interval(Duration::from_millis(50)),
        );
        let found = charter_core::files::root(
            root,
            charter_core::files::Branch::piece("alpha", "thing", "piece"),
        )
        .unwrap();
        let chosen = how(&found);
        watch
            .set_from("main", watch.ticket(), vec![(branch(), found, chosen)])
            .unwrap();
        // The poller's first look is its baseline; give it one.
        std::thread::sleep(Duration::from_millis(300));
        (watch, rx)
    }

    fn both_ways() -> [fn(&Root) -> How; 2] {
        [|_| How::Whole, |root| How::Folders(root.folders().unwrap())]
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

    #[test]
    fn a_folder_made_in_the_branch_is_listened_in_once_it_appears() {
        let (_dir, root, piece) = plane();
        let (watch, told) = listening(&root, |root| How::Folders(root.folders().unwrap()));
        let new = std::fs::canonicalize(&piece).unwrap().join("src/new");
        assert!(!watch.watching().contains(&new));

        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("a.rs"), "\n").unwrap();

        told.recv_timeout(PATIENCE)
            .expect("the new folder was heard");
        assert!(watch.watching().contains(&new), "{:?}", watch.watching());
    }
}
