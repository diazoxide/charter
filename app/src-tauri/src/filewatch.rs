//! Tells a window when a folder its explorer has expanded changes on disk (FM-1, #1103 story
//! 57): an agent adds or removes a file, and the tree shows it without a manual refresh.
//!
//! **The same watcher as the plane's (`planewatch.rs`), on different folders.** The plane watch
//! deliberately stays out of the clones: a recursive watch there would put one on every folder
//! of every `node_modules/` and `target/`. This one watches exactly the folders a window has
//! expanded, each **non-recursively**, so its cost is what the operator has open and nothing
//! below it. A window hands its whole set each time it changes, and the set is dropped with the
//! window.
//!
//! **The window names folders, never directories.** Each one is a branch and a path inside it,
//! resolved by `charter_core::files::folders` with every check the tree's own read makes; one
//! that does not resolve is not watched. Each branch is found once per call, however many of
//! its folders are open, and at most `charter_core::files::WATCHED` folders are watched for a
//! window.
//!
//! **One event per batch, debounced** as the plane's is, and an access is not a change: the
//! window reads the folder again when told, and on inotify that read is an `IN_OPEN`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer_opt};

use crate::planes::{PlaneId, Planes};

/// The event a window is sent.
pub(crate) const CHANGED: &str = "files-changed";

/// How long a burst is folded for: an agent writing ten files is one read of their folder.
const QUIET_FOR: Duration = Duration::from_millis(250);

/// One folder of a branch, as the window names it: the plane, the workspace, the repo, the piece
/// (none for the repo's own folder) and the folder's path inside the branch (`""` for its top).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct BranchFolder {
    pub plane: PlaneId,
    pub workspace: String,
    pub repo: String,
    pub piece: Option<String>,
    pub folder: String,
}

/// What `files-changed` carries: the folders, of those this window asked to watch, that moved.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct FilesChanged {
    pub folders: Vec<BranchFolder>,
}

/// Told which window's folders moved.
pub type Told = Arc<dyn Fn(&str, Vec<BranchFolder>) + Send + Sync + 'static>;

struct Inner<W: notify::Watcher> {
    debouncer: Option<Debouncer<W, RecommendedCache>>,
    /// Each window's folders, with the directory each resolved to.
    by_window: HashMap<String, Vec<(BranchFolder, PathBuf)>>,
    /// The newest set each window asked for, by its [`FileWatch::ticket`]: a set resolved late
    /// never replaces one asked for after it.
    newest: HashMap<String, u64>,
    watched: HashSet<PathBuf>,
}

/// Every window's watched folders. Managed by the app; dropping it stops every watch.
pub struct FileWatch<W: notify::Watcher = notify::RecommendedWatcher> {
    inner: Arc<Mutex<Inner<W>>>,
    told: Told,
    config: notify::Config,
    tickets: std::sync::atomic::AtomicU64,
}

impl FileWatch {
    /// A watch that tells `told`, on the platform's own watcher. Nothing is watched until a
    /// window asks.
    pub fn new(told: Told) -> Self {
        Self::with_config(told, notify::Config::default())
    }
}

impl<W: notify::Watcher + Send + 'static> FileWatch<W> {
    fn with_config(told: Told, config: notify::Config) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                debouncer: None,
                by_window: HashMap::new(),
                newest: HashMap::new(),
                watched: HashSet::new(),
            })),
            told,
            config,
            tickets: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// A number for a set a window asks for, larger than every one handed out before: taken
    /// when the ask arrives, so the order of the asks decides and not the order their folders
    /// finish resolving in.
    pub fn ticket(&self) -> u64 {
        self.tickets
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(1)
    }

    /// [`FileWatch::set`], unless the window has since asked for a newer set.
    pub fn set_from(
        &self,
        window: &str,
        ticket: u64,
        folders: Vec<(BranchFolder, PathBuf)>,
    ) -> Result<(), String> {
        {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let newest = inner.newest.entry(window.to_string()).or_default();
            if ticket < *newest {
                return Ok(());
            }
            *newest = ticket;
        }
        self.set(window, folders)
    }

    /// `window`'s folders are now `folders`, each with the directory it resolved to; the ones
    /// it had before and does not name are no longer watched for it.
    pub fn set(&self, window: &str, folders: Vec<(BranchFolder, PathBuf)>) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if folders.is_empty() {
            inner.by_window.remove(window);
        } else {
            inner.by_window.insert(window.to_string(), folders);
        }
        if inner.debouncer.is_none() && !inner.by_window.is_empty() {
            inner.debouncer = Some(self.start()?);
        }
        inner.follow();
        Ok(())
    }

    /// `window` is gone: none of its folders is watched for it any more.
    pub fn forget(&self, window: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.newest.remove(window);
        if inner.by_window.remove(window).is_some() {
            inner.follow();
        }
    }

    fn start(&self) -> Result<Debouncer<W, RecommendedCache>, String> {
        let handle: Weak<Mutex<Inner<W>>> = Arc::downgrade(&self.inner);
        let told = Arc::clone(&self.told);
        let tell = move |batch: DebounceEventResult| {
            let Ok(events) = batch else { return };
            // A path that changed, and the folder it changed in: a file added is an event on
            // the file, and a watched folder removed is an event on the folder itself.
            let mut moved: HashSet<&Path> = HashSet::new();
            for event in events
                .iter()
                .filter(|event| crate::watchset::matters(&event.kind))
            {
                for path in &event.paths {
                    moved.insert(path);
                    if let Some(parent) = path.parent() {
                        moved.insert(parent);
                    }
                }
            }
            if moved.is_empty() {
                return;
            }
            let Some(inner) = handle.upgrade() else {
                return;
            };
            let concerned: Vec<(String, Vec<BranchFolder>)> = {
                let inner = inner.lock().unwrap_or_else(PoisonError::into_inner);
                inner
                    .by_window
                    .iter()
                    .filter_map(|(window, folders)| {
                        let hit: Vec<BranchFolder> = folders
                            .iter()
                            .filter(|(_, dir)| moved.contains(dir.as_path()))
                            .map(|(folder, _)| folder.clone())
                            .collect();
                        (!hit.is_empty()).then(|| (window.clone(), hit))
                    })
                    .collect()
            };
            // Outside the lock, so a window that answers by watching again never waits on it.
            for (window, folders) in concerned {
                told(&window, folders);
            }
        };
        new_debouncer_opt(QUIET_FOR, None, tell, RecommendedCache::new(), self.config)
            .map_err(|e| format!("charter could not watch the branch's folders: {e}"))
    }
}

impl<W: notify::Watcher> Inner<W> {
    /// Watches every window's folders, and stops watching the ones no window names.
    fn follow(&mut self) {
        let wanted: HashSet<PathBuf> = self
            .by_window
            .values()
            .flatten()
            .map(|(_, dir)| dir.clone())
            .collect();
        if let Some(debouncer) = self.debouncer.as_mut() {
            crate::watchset::follow(debouncer, &mut self.watched, wanted);
        }
    }
}

/// The folders of branches this window's explorer has expanded, watched until it names others
/// (FM-1). A folder that does not resolve — gone, or refused as the tree refuses it — is not
/// watched.
// Each folder is resolved by `charter_core::files::folder`, with the tree's own checks; the
// window never names a directory. Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn files_watch(
    window: tauri::Window,
    planes: tauri::State<'_, Planes>,
    folders: Vec<BranchFolder>,
) -> Result<(), String> {
    use tauri::Manager as _;
    let ticket = window.state::<FileWatch>().ticket();
    // Each project resolved here, so the blocking half carries paths and not registry handles;
    // and each branch once, with every one of its folders, and no more than a window may watch.
    let mut by_branch: Vec<(PathBuf, Vec<BranchFolder>)> = Vec::new();
    for folder in folders.into_iter().take(charter_core::files::WATCHED) {
        let Ok(held) = planes.held(&folder.plane) else {
            continue;
        };
        match by_branch
            .iter_mut()
            .find(|(_, of)| same_branch(&of[0], &folder))
        {
            Some((_, of)) => of.push(folder),
            None => by_branch.push((held.root().to_path_buf(), vec![folder])),
        }
    }
    // Off the thread that draws (SC-2): finding a branch's folder asks git for its worktrees.
    let resolved = tauri::async_runtime::spawn_blocking(move || {
        let mut resolved = Vec::new();
        for (root, of) in by_branch {
            let first = &of[0];
            let branch = crate::piecefiles::branch(&first.workspace, &first.repo, &first.piece);
            let named: Vec<&str> = of.iter().map(|folder| folder.folder.as_str()).collect();
            let dirs = charter_core::files::folders(&root, branch, &named);
            for (folder, dir) in of.iter().zip(dirs) {
                if let Ok(dir) = dir {
                    resolved.push((folder.clone(), dir));
                }
            }
        }
        resolved
    })
    .await
    .map_err(|err| format!("watching the branch's folders did not finish: {err}"))?;
    window
        .state::<FileWatch>()
        .set_from(window.label(), ticket, resolved)
}

/// Whether two folders are of one branch of one project.
fn same_branch(a: &BranchFolder, b: &BranchFolder) -> bool {
    a.plane == b.plane && a.workspace == b.workspace && a.repo == b.repo && a.piece == b.piece
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const PATIENCE: Duration = Duration::from_secs(10);

    /// The poller on macOS, where FSEvents gives no bound on when it delivers (#577); the
    /// platform's own watcher elsewhere. As `planewatch.rs`'s tests choose.
    #[cfg(target_os = "macos")]
    type Source = notify::PollWatcher;
    #[cfg(not(target_os = "macos"))]
    type Source = notify::RecommendedWatcher;

    fn folder(name: &str) -> BranchFolder {
        BranchFolder {
            plane: serde_json::from_value(serde_json::json!("/plane")).unwrap(),
            workspace: "alpha".to_string(),
            repo: "thing".to_string(),
            piece: Some("piece".to_string()),
            folder: name.to_string(),
        }
    }

    fn watching() -> (
        FileWatch<Source>,
        mpsc::Receiver<(String, Vec<BranchFolder>)>,
    ) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = FileWatch::<Source>::with_config(
            Arc::new(move |window: &str, folders| {
                let _ = tx.lock().unwrap().send((window.to_string(), folders));
            }),
            notify::Config::default().with_poll_interval(Duration::from_millis(50)),
        );
        (watch, rx)
    }

    #[test]
    fn a_file_added_to_an_expanded_folder_tells_the_window_that_expanded_it() {
        let dir = tempfile::tempdir().unwrap();
        let src = std::fs::canonicalize(dir.path()).unwrap().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let (watch, told) = watching();
        watch
            .set("main", vec![(folder("src"), src.clone())])
            .unwrap();
        // The poller's first look is its baseline; give it one.
        std::thread::sleep(Duration::from_millis(200));

        std::fs::write(src.join("new.rs"), "\n").unwrap();

        let (window, folders) = told.recv_timeout(PATIENCE).expect("the window was told");
        assert_eq!(window, "main");
        assert_eq!(folders, [folder("src")]);
    }

    #[test]
    fn a_set_resolved_late_never_replaces_one_asked_for_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let src = std::fs::canonicalize(dir.path()).unwrap().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let (watch, told) = watching();
        let (older, newer) = (watch.ticket(), watch.ticket());
        watch
            .set_from("main", newer, vec![(folder("src"), src.clone())])
            .unwrap();
        watch.set_from("main", older, Vec::new()).unwrap();
        std::thread::sleep(Duration::from_millis(200));

        std::fs::write(src.join("new.rs"), "\n").unwrap();

        let (_, folders) = told
            .recv_timeout(PATIENCE)
            .expect("the newer set is watched");
        assert_eq!(folders, [folder("src")]);
    }

    #[test]
    fn a_folder_no_window_names_any_more_tells_nobody() {
        let dir = tempfile::tempdir().unwrap();
        let src = std::fs::canonicalize(dir.path()).unwrap().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let (watch, told) = watching();
        watch
            .set("main", vec![(folder("src"), src.clone())])
            .unwrap();
        watch.forget("main");
        std::thread::sleep(Duration::from_millis(200));

        std::fs::write(src.join("new.rs"), "\n").unwrap();

        assert!(told.recv_timeout(Duration::from_secs(2)).is_err());
    }
}
