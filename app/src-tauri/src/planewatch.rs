//! Tells the window when a plane changes on disk under it (charter-app#264).
//!
//! `charter ws todo done` in a terminal deletes a todo's file, and the window's Todos panel
//! went on drawing it for hours: the window read a workspace once, when it was focused, and
//! nothing ever said the plane had moved. The plane is a directory that the CLI, other chats
//! and the operator's editor all write, so a cache of it can only be kept honest by being told.
//!
//! # What is watched, and why it is not the whole plane
//!
//! The directories the panels and the sidebar read, each **non-recursively**:
//!
//! - the plane root (`charter.toml`, which names the default persona, and `workspaces/`
//!   itself appearing);
//! - `workspaces/`, so a workspace made or deleted elsewhere is seen and watched in turn;
//! - each `workspaces/<ws>/` (`workspace.json`, a clone arriving, `todos/` being created);
//! - each `workspaces/<ws>/todos/`, which is the bug;
//! - `personas/`, and each `personas/<name>/`, whose `persona.md` a chat reads at its start;
//! - `.claude/` and `.claude/agents/`, the harness settings and sub-agents a chat reads at its
//!   start — with the root's `CLAUDE.md`, what the window marks a chat for when it changes under
//!   it (charter#369, [`charter_core::instructions`]).
//!
//! Not the plane recursively, because a workspace holds its clones: a recursive inotify watch
//! would put one watch on every directory of every clone's `node_modules/` and `target/`, and
//! run the machine out of watches. (On macOS FSEvents streams the subtree regardless and notify
//! drops what is not a direct child in-process, so there the narrow set is about what is
//! REPORTED, not what is watched.) The set is re-read after every batch, so a workspace made
//! in a terminal is watched from then on.
//!
//! # One event, debounced
//!
//! `notify-debouncer-full` folds a burst — `git pull` landing ten todos, an editor's
//! write-then-rename — into one batch, and a batch is one `plane-changed` carrying the plane.
//! The window reads the plane again on it; it is never told *what* changed, because the answer
//! to that is the plane, and the window already knows how to read it.
//!
//! **An access is not a change.** notify's inotify backend watches `IN_OPEN`, and reading a
//! todo opens it — so a window that re-read on every event would re-read because it re-read,
//! forever. [`matters`] is where that loop is cut.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use charter_core::workspaces::Plane;
use notify::event::{MetadataKind, ModifyKind};
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::planes::PlaneId;

/// The event the window is sent.
pub(crate) const CHANGED: &str = "plane-changed";

/// What `plane-changed` carries: which plane moved. Every window filters on it, as it filters
/// `chat-moved`, because the app holds several planes and emits on the app.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct PlaneChanged {
    pub plane: PlaneId,
}

/// Told whenever a plane changes on disk, whichever plane it is.
pub type Changed = Arc<dyn Fn(PlaneId) + Send + Sync + 'static>;

/// How long a burst is folded for. Short enough that a todo closed in a terminal is off the
/// panel within the second #264 asks for; long enough that a `git pull` is one read, not ten.
const QUIET_FOR: Duration = Duration::from_millis(250);

type Watcher = Debouncer<notify::RecommendedWatcher, RecommendedCache>;

/// The watcher and the directories it is watching, behind one lock: the set is re-read on the
/// watcher's own thread after every batch.
struct Inner {
    debouncer: Option<Watcher>,
    watched: HashSet<PathBuf>,
}

/// One plane's watch. Dropping it stops it.
pub struct Watch {
    inner: Arc<Mutex<Inner>>,
}

impl Watch {
    /// Starts watching `root` and tells `changed` about `plane` whenever it moves.
    pub fn start(plane: PlaneId, root: &Path, changed: Changed) -> notify::Result<Self> {
        let inner = Arc::new(Mutex::new(Inner {
            debouncer: None,
            watched: HashSet::new(),
        }));
        let handle: Weak<Mutex<Inner>> = Arc::downgrade(&inner);
        let at = root.to_path_buf();
        let debouncer = new_debouncer(QUIET_FOR, None, move |batch: DebounceEventResult| {
            let Ok(events) = batch else { return };
            let events: Vec<_> = events.iter().filter(|event| matters(&event.kind)).collect();
            if events.is_empty() {
                return;
            }
            // The set first, so a workspace made in this batch is watched before the window
            // reads it, and a change inside it straight after is not missed.
            //
            // **And nothing at all once the watch is dropped.** The debouncer's drop only asks
            // its thread to stop; a batch it had already gathered still arrives here, and a
            // plane that has been closed must not be reported.
            {
                let Some(inner) = handle.upgrade() else {
                    return;
                };
                let mut inner = inner.lock().unwrap_or_else(PoisonError::into_inner);
                if inner.debouncer.is_none() {
                    return;
                }
                for event in &events {
                    if matches!(event.kind, EventKind::Remove(_)) {
                        // Gone, so its watch is gone with it (inotify drops it on its own). A
                        // directory removed and made again in one batch has to be watched
                        // again, so it must not still be counted as watched.
                        for path in &event.paths {
                            inner.forget(path);
                        }
                    }
                }
                inner.follow(&at);
            }
            changed(plane.clone());
        })?;
        {
            let mut held = inner.lock().unwrap_or_else(PoisonError::into_inner);
            held.debouncer = Some(debouncer);
            held.follow(root);
        }
        Ok(Self { inner })
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // Taken out under the lock and dropped outside it, so the watcher's thread, which takes
        // the same lock, never waits on a drop. An empty slot is also what tells a batch
        // already in flight that nobody is listening any more.
        let debouncer = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .debouncer
            .take();
        drop(debouncer);
    }
}

impl Inner {
    /// Watches what the plane has now, and stops watching what it no longer has.
    fn follow(&mut self, root: &Path) {
        let Some(debouncer) = self.debouncer.as_mut() else {
            return;
        };
        let wanted = wanted(root);
        let stale: Vec<PathBuf> = self.watched.difference(&wanted).cloned().collect();
        for path in stale {
            // An error is a watch the platform has already dropped with its directory.
            let _ = debouncer.unwatch(&path);
            self.watched.remove(&path);
        }
        for path in wanted {
            if self.watched.contains(&path) {
                continue;
            }
            // A directory that went between the listing and here is simply not watched; the
            // next batch lists again.
            if debouncer.watch(&path, RecursiveMode::NonRecursive).is_ok() {
                self.watched.insert(path);
            }
        }
    }

    fn forget(&mut self, path: &Path) {
        if self.watched.remove(path)
            && let Some(debouncer) = self.debouncer.as_mut()
        {
            let _ = debouncer.unwatch(path);
        }
    }
}

/// Whether an event is a change to the plane. Everything but an access is: reading a file
/// changes nothing, and it is exactly what the window does when it is told. An access time
/// moving is the same read seen from inotify's `IN_ATTRIB` under `relatime`.
fn matters(kind: &EventKind) -> bool {
    !matches!(
        kind,
        EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime))
    )
}

/// The directories the panels and the sidebar read, as they are on disk now. See the module's
/// header for why these and not the plane recursively.
///
/// **A link is not followed.** charter refuses a store that is a link out of the plane, and a
/// watch through one would be charter listening to a directory it will not read.
fn wanted(root: &Path) -> HashSet<PathBuf> {
    let a_dir =
        |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|found| found.file_type().is_dir());
    let mut wanted = HashSet::new();
    if a_dir(root) {
        wanted.insert(root.to_path_buf());
    }
    let personas = root.join("personas");
    if a_dir(&personas) {
        for name in std::fs::read_dir(&personas).into_iter().flatten().flatten() {
            let persona = name.path();
            if a_dir(&persona) {
                wanted.insert(persona);
            }
        }
        wanted.insert(personas);
    }
    for harness in [".claude", ".claude/agents"] {
        let dir = root.join(harness);
        if a_dir(&dir) {
            wanted.insert(dir);
        }
    }
    let workspaces = root.join("workspaces");
    if !a_dir(&workspaces) {
        return wanted;
    }
    wanted.insert(workspaces);
    let plane = Plane::open(root);
    for name in plane.workspaces().unwrap_or_default() {
        let Ok(workspace) = plane.workspace(&name) else {
            continue;
        };
        let dir = workspace.dir().to_path_buf();
        if !a_dir(&dir) {
            continue;
        }
        let todos = dir.join("todos");
        if a_dir(&todos) {
            wanted.insert(todos);
        }
        wanted.insert(dir);
    }
    wanted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    /// Long enough for a loaded CI runner; the acceptance is "about a second" and a pass
    /// comes back in a quarter of one.
    const PATIENCE: Duration = Duration::from_secs(10);

    fn plane_with_todos(todos: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a scratch plane");
        let store = dir.path().join("workspaces/alpha/todos");
        std::fs::create_dir_all(&store).expect("a todo store");
        std::fs::create_dir_all(dir.path().join("personas")).expect("personas");
        for slug in todos {
            std::fs::write(store.join(format!("{slug}.md")), format!("# {slug}\n")).expect("todo");
        }
        dir
    }

    fn id(root: &Path) -> PlaneId {
        serde_json::from_value(serde_json::json!(root.display().to_string())).expect("an id")
    }

    /// A watch on `root` and the channel it tells.
    fn watching(root: &Path) -> (Watch, mpsc::Receiver<PlaneId>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = Watch::start(
            id(root),
            root,
            Arc::new(move |plane| {
                let _ = tx.lock().expect("sender").send(plane);
            }),
        )
        .expect("a watch");
        // FSEvents reports what happened a moment BEFORE the stream started, on some
        // machines; a test that changed the plane at once could be told about its own setup.
        std::thread::sleep(Duration::from_millis(300));
        while rx.try_recv().is_ok() {}
        (watch, rx)
    }

    #[test]
    fn a_todo_file_removed_under_a_watched_plane_is_told_within_the_second() {
        let plane = plane_with_todos(&["m8-1", "m8-2"]);
        let root = plane.path().canonicalize().expect("canonical");
        let (_watch, told) = watching(&root);

        let slugs = || {
            let read = crate::panels::of(&root, "alpha").expect("the panels read");
            let read = serde_json::to_value(read).expect("serialisable");
            read["todos"]
                .as_array()
                .expect("todos")
                .iter()
                .map(|todo| todo["slug"].as_str().expect("a slug").to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(slugs(), ["m8-1", "m8-2"]);

        let started = Instant::now();
        std::fs::remove_file(root.join("workspaces/alpha/todos/m8-1.md")).expect("closed");

        let plane = told.recv_timeout(PATIENCE).expect("told the plane changed");
        // #264's "within about a second": the debounce is a quarter of one, and two is the
        // margin a loaded runner gets.
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "told after {:?}",
            started.elapsed()
        );
        assert_eq!(plane, id(&root));
        // And what the window reads when told — the same command the Todos panel draws from —
        // no longer has the row.
        assert_eq!(slugs(), ["m8-2"]);
    }

    #[test]
    fn a_burst_of_changes_is_folded_rather_than_told_per_write() {
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        let (_watch, told) = watching(&root);

        for n in 0..10 {
            std::fs::write(
                root.join(format!("workspaces/alpha/todos/t{n}.md")),
                "# t\n",
            )
            .expect("a todo");
        }

        told.recv_timeout(PATIENCE).expect("told");
        // Once, and twice at most on a loaded runner where the burst straddles a window —
        // never once per write.
        let more = std::iter::from_fn(|| told.recv_timeout(QUIET_FOR * 4).ok()).count();
        assert!(
            more <= 1,
            "a burst of ten writes was told {} times",
            more + 1
        );
    }

    #[test]
    fn a_todo_in_a_workspace_made_after_the_watch_began_is_still_told() {
        // The workspace comes from a terminal while the window is open: `workspaces/` moves,
        // the set is re-read, and the new workspace's store is watched from then on.
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        let (_watch, told) = watching(&root);

        std::fs::create_dir_all(root.join("workspaces/beta/todos")).expect("a new workspace");
        told.recv_timeout(PATIENCE)
            .expect("told about the workspace");
        // Settle, then drain whatever the creation's tail produced.
        std::thread::sleep(QUIET_FOR * 2);
        while told.try_recv().is_ok() {}

        std::fs::write(root.join("workspaces/beta/todos/new.md"), "# new\n").expect("a todo");
        told.recv_timeout(PATIENCE)
            .expect("told about a todo in the new workspace");
    }

    #[test]
    fn nothing_is_told_once_the_watch_is_dropped() {
        let plane = plane_with_todos(&["m8-1"]);
        let root = plane.path().canonicalize().expect("canonical");
        let (watch, told) = watching(&root);
        drop(watch);

        std::fs::remove_file(root.join("workspaces/alpha/todos/m8-1.md")).expect("closed");
        assert!(told.recv_timeout(QUIET_FOR * 4).is_err());
    }

    #[test]
    fn reading_a_todo_is_not_a_change() {
        // The loop inotify would otherwise close: the window reads because it was told, and
        // the read opens the files it was told about.
        use notify::event::{AccessKind, AccessMode, CreateKind, RemoveKind};
        assert!(!matters(&EventKind::Access(AccessKind::Open(
            AccessMode::Read
        ))));
        assert!(!matters(&EventKind::Access(AccessKind::Close(
            AccessMode::Read
        ))));
        assert!(!matters(&EventKind::Modify(ModifyKind::Metadata(
            MetadataKind::AccessTime
        ))));
        assert!(matters(&EventKind::Remove(RemoveKind::File)));
        assert!(matters(&EventKind::Create(CreateKind::File)));
    }

    #[test]
    fn the_watched_set_is_the_stores_the_panels_read_and_not_the_clones() {
        let plane = plane_with_todos(&[]);
        let root = plane.path();
        std::fs::create_dir_all(root.join("workspaces/alpha/svc/.git")).expect("a clone");
        std::fs::create_dir_all(root.join("workspaces/alpha/svc/src")).expect("its source");
        std::fs::create_dir_all(root.join("workspaces/.worktrees")).expect("charter's own");
        std::fs::create_dir_all(root.join("personas/steward/memory")).expect("a persona");
        std::fs::create_dir_all(root.join(".claude/agents")).expect("sub-agents");
        std::fs::create_dir_all(root.join(".claude/skills/x")).expect("a skill");

        let mut got: Vec<_> = wanted(root)
            .into_iter()
            .map(|path| {
                path.strip_prefix(root)
                    .expect("inside")
                    .display()
                    .to_string()
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                "",
                ".claude",
                ".claude/agents",
                "personas",
                "personas/steward",
                "workspaces",
                "workspaces/alpha",
                "workspaces/alpha/todos"
            ]
        );
    }
}
