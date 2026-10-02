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
//! - each `workspaces/<ws>/memory/` and `workspaces/<ws>/sessions/`, and the root's
//!   `sessions/`: the Memory and Sessions panels' stores, so a memory an agent saves or a
//!   session record a smart close writes reaches the panel without some other change having
//!   to happen first (FD-10);
//! - `personas/`, and each `personas/<name>/`, whose `persona.md` a chat reads at its start,
//!   and each `personas/<name>/memory/`, which the Personas panel counts;
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
//! write-then-rename — into one batch, and a batch is one `plane-changed` carrying the plane
//! **and what changed in it**: each path, and what it is part of
//! ([`charter_core::planechange`]) — a todo of `alpha`, a memory of `steward`, the root's
//! session records. A panel reads again only on a change its answer is made of (FD-10), so a
//! memory an agent saves no longer makes the sidebar list every workspace's todos once more.
//!
//! A batch this cannot place — a path outside the plane, an event notify could not name a path
//! for, a rescan — is told as `changes: null`, "anything may have moved", and every reader
//! reads again, as each one did before there were kinds.
//!
//! **An access is not a change.** notify's inotify backend watches `IN_OPEN`, and reading a
//! todo opens it — so a window that re-read on every event would re-read because it re-read,
//! forever. [`matters`] is where that loop is cut.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use charter_core::planechange::{self, Answer, Change, Kind};
use charter_core::workspaces::Plane;
use notify::event::{MetadataKind, ModifyKind};
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer_opt};

use crate::planes::PlaneId;

/// The event the window is sent.
pub(crate) const CHANGED: &str = "plane-changed";

/// What `plane-changed` carries: which plane moved, and what moved in it. Every window filters
/// on the plane, as it filters `chat-moved`, because the app holds several planes and emits on
/// the app.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct PlaneChanged {
    pub plane: PlaneId,
    /// Each changed path and what it is part of, or `null` when what changed is not known —
    /// a batch this could not place — and every reader reads again. Auto-save says what it
    /// did as one change of kind `git`, with no path.
    pub changes: Option<Vec<PlaneChange>>,
    /// The answers these changes concern, each once, or `null` — every answer — when what
    /// changed is not known. A reader names the answer it holds and reads again only when it
    /// is here: which answer a change concerns is the core's question
    /// ([`charter_core::planechange::answers`]), never the window's.
    pub answers: Option<Vec<PlaneAnswer>>,
}

impl PlaneChanged {
    /// What the window is told about `plane` when `changes` moved in it.
    pub fn of(plane: PlaneId, changes: What) -> Self {
        let answers = planechange::answers(changes.as_deref())
            .map(|answers| answers.into_iter().map(PlaneAnswer::from).collect());
        Self {
            plane,
            changes: changes.map(|changes| changes.into_iter().map(PlaneChange::from).collect()),
            answers,
        }
    }
}

/// One answer the window reads from the plane ([`charter_core::planechange::Answer`],
/// mirrored for the bindings).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "answer", rename_all = "camelCase")]
pub enum PlaneAnswer {
    /// `plane_sidebar`.
    Sidebar,
    /// `workspace_panels` for `workspace`, or for every workspace when it is `null`.
    Panels { workspace: Option<String> },
    /// `plane_root_panels`.
    RootPanels,
    /// The plane's shape beside its panels: the instructions a chat started on, the curations.
    Shape,
    /// What the project has on, and its theme.
    Settings,
    /// The git standings: the alerts and the Saving rows.
    Git,
    /// The view tabs.
    Views,
}

impl From<Answer> for PlaneAnswer {
    fn from(answer: Answer) -> Self {
        match answer {
            Answer::Sidebar => Self::Sidebar,
            Answer::Panels { workspace } => Self::Panels { workspace },
            Answer::RootPanels => Self::RootPanels,
            Answer::Shape => Self::Shape,
            Answer::Settings => Self::Settings,
            Answer::Git => Self::Git,
            Answer::Views => Self::Views,
        }
    }
}

/// What one changed path is part of, as the window's readers divide the plane
/// ([`charter_core::planechange::Kind`], which this mirrors for the bindings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Project,
    Harness,
    Workspace,
    Todos,
    Memory,
    Sessions,
    Persona,
    Git,
}

/// One changed path ([`charter_core::planechange::Change`], mirrored for the bindings).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct PlaneChange {
    pub kind: ChangeKind,
    /// The workspace it is in, where it is in one.
    pub workspace: Option<String>,
    /// The persona it belongs to, where it belongs to one (`_shared` for the shared store).
    pub persona: Option<String>,
    /// Relative to the plane root, `/` between its parts.
    pub path: String,
}

impl From<Change> for PlaneChange {
    fn from(change: Change) -> Self {
        Self {
            kind: match change.kind {
                Kind::Project => ChangeKind::Project,
                Kind::Harness => ChangeKind::Harness,
                Kind::Workspace => ChangeKind::Workspace,
                Kind::Todos => ChangeKind::Todos,
                Kind::Memory => ChangeKind::Memory,
                Kind::Sessions => ChangeKind::Sessions,
                Kind::Persona => ChangeKind::Persona,
                Kind::Git => ChangeKind::Git,
            },
            workspace: change.workspace,
            persona: change.persona,
            path: change.path,
        }
    }
}

/// What a plane change says moved: the changes, or `None` for "not known, read everything".
pub type What = Option<Vec<Change>>;

/// Told whenever a plane changes on disk, whichever plane it is, and what changed in it.
pub type Changed = Arc<dyn Fn(PlaneId, What) + Send + Sync + 'static>;

/// How long a burst is folded for. Short enough that a todo closed in a terminal is off the
/// panel within the second #264 asks for; long enough that a `git pull` is one read, not ten.
const QUIET_FOR: Duration = Duration::from_millis(250);

/// The watcher and the directories it is watching, behind one lock: the set is re-read on the
/// watcher's own thread after every batch.
struct Inner<W: notify::Watcher> {
    debouncer: Option<Debouncer<W, RecommendedCache>>,
    watched: HashSet<PathBuf>,
}

/// One plane's watch. Dropping it stops it.
///
/// `W` is where the changes come from: the platform's own watcher (FSEvents, inotify) in the
/// app. The tests on macOS choose notify's poller instead, because FSEvents gives no bound on
/// when it delivers (#577): its one daemon serves the whole machine, and on a Mac busy writing
/// build trees it was measured handing a stream a change 4 to 15 seconds late, and a stream
/// nothing at all for minutes. What this module decides — fold a burst, follow a new workspace,
/// fall silent when dropped — is the same whichever watcher feeds it.
pub struct Watch<W: notify::Watcher = notify::RecommendedWatcher> {
    inner: Arc<Mutex<Inner<W>>>,
}

impl Watch {
    /// Starts watching `root` and tells `changed` about `plane` whenever it moves.
    pub fn start(plane: PlaneId, root: &Path, changed: Changed) -> notify::Result<Self> {
        Self::start_with(plane, root, changed, notify::Config::default())
    }
}

impl<W: notify::Watcher + Send + 'static> Watch<W> {
    /// [`Watch::start`] on a watcher of the caller's choosing, configured by `config`.
    fn start_with(
        plane: PlaneId,
        root: &Path,
        changed: Changed,
        config: notify::Config,
    ) -> notify::Result<Self> {
        let inner = Arc::new(Mutex::new(Inner {
            debouncer: None,
            watched: HashSet::new(),
        }));
        let handle: Weak<Mutex<Inner<W>>> = Arc::downgrade(&inner);
        let at = root.to_path_buf();
        // The platform reports paths as the disk spells them, which a root opened through a
        // link (`/var` for `/private/var` on macOS) is not.
        let spelled = root.canonicalize().ok();
        let tell = move |batch: DebounceEventResult| {
            let Ok(events) = batch else { return };
            let events: Vec<_> = events.iter().filter(|event| matters(&event.kind)).collect();
            if events.is_empty() {
                return;
            }
            let what = what_changed(&at, spelled.as_deref(), &events);
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
            // Before the window hears it, so the reads it makes on `plane-changed` are of the
            // plane as it is now, not the shared standings from before (FD-11).
            charter_core::planegit::touch(&at);
            changed(plane.clone(), what);
        };
        let debouncer = new_debouncer_opt(QUIET_FOR, None, tell, RecommendedCache::new(), config)?;
        {
            let mut held = inner.lock().unwrap_or_else(PoisonError::into_inner);
            held.debouncer = Some(debouncer);
            held.follow(root);
        }
        Ok(Self { inner })
    }
}

impl<W: notify::Watcher> Drop for Watch<W> {
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

impl<W: notify::Watcher> Inner<W> {
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

/// What a batch changed, each path placed by [`planechange`] against the root as it was given
/// or, failing that, as the disk spells it — or `None` when any one of them cannot be placed,
/// an event names no path, or notify asks for a rescan (it dropped events it cannot name).
fn what_changed(
    root: &Path,
    spelled: Option<&Path>,
    events: &[&notify_debouncer_full::DebouncedEvent],
) -> What {
    if events
        .iter()
        .any(|event| event.paths.is_empty() || event.need_rescan())
    {
        return None;
    }
    let paths = || {
        events
            .iter()
            .flat_map(|event| event.paths.iter().map(PathBuf::as_path))
    };
    planechange::of_batch(root, paths())
        .or_else(|| spelled.and_then(|spelled| planechange::of_batch(spelled, paths())))
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
                let memory = persona.join("memory");
                if a_dir(&memory) {
                    wanted.insert(memory);
                }
                wanted.insert(persona);
            }
        }
        wanted.insert(personas);
    }
    let records = root.join("sessions");
    if a_dir(&records) {
        wanted.insert(records);
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
        for store in ["todos", "memory", "sessions"] {
            let store = dir.join(store);
            if a_dir(&store) {
                wanted.insert(store);
            }
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

    /// How long a test waits to be told before it fails. Only a failure waits this long; a
    /// pass comes back in a quarter of a second.
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

    /// Where the tests' changes come from (#577). On macOS, notify's poller: FSEvents has no
    /// bound on when it delivers. On a busy Mac it handed a stream a change seconds to minutes
    /// late, and handed it the writes that made the plane seconds after the stream began, so
    /// no deadline made these tests pass there; a longer one only waited longer for the same
    /// flake. Elsewhere the platform's own watcher, as the app runs it: inotify is where an
    /// access used to loop.
    #[cfg(target_os = "macos")]
    type Source = notify::PollWatcher;
    #[cfg(not(target_os = "macos"))]
    type Source = notify::RecommendedWatcher;

    /// How often the poller looks; inotify ignores it. Well inside [`QUIET_FOR`], so what the
    /// window waits on is the debounce, not the look.
    const LOOK_EVERY: Duration = Duration::from_millis(50);

    /// A watch on `root` fed by the tests' [`Source`], and the channel it tells. Anything done
    /// to the plane from here on is told. Nothing done before it is — a promise of the poller
    /// and of inotify, NOT of FSEvents, which the app runs on macOS (see [`Source`]).
    fn watching(root: &Path) -> (Watch<Source>, mpsc::Receiver<PlaneId>) {
        watching_on::<Source>(root)
    }

    /// [`watching`] on a watcher of the test's choosing.
    fn watching_on<W: notify::Watcher + Send + 'static>(
        root: &Path,
    ) -> (Watch<W>, mpsc::Receiver<PlaneId>) {
        let (watch, told) = telling_on::<W>(root);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (plane, _) in told {
                if tx.send(plane).is_err() {
                    return;
                }
            }
        });
        (watch, rx)
    }

    /// [`watching`], told what changed as well as which plane.
    fn telling(root: &Path) -> (Watch<Source>, mpsc::Receiver<(PlaneId, What)>) {
        telling_on::<Source>(root)
    }

    fn telling_on<W: notify::Watcher + Send + 'static>(
        root: &Path,
    ) -> (Watch<W>, mpsc::Receiver<(PlaneId, What)>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = Watch::<W>::start_with(
            id(root),
            root,
            Arc::new(move |plane, what| {
                let _ = tx.lock().expect("sender").send((plane, what));
            }),
            notify::Config::default().with_poll_interval(LOOK_EVERY),
        )
        .expect("a watch");
        (watch, rx)
    }

    #[test]
    fn a_todo_file_removed_under_a_watched_plane_is_told_within_the_second() {
        a_removed_todo_is_told_within_the_second_on::<Source>();
    }

    /// The same on the watcher the app runs on macOS. Ignored because FSEvents gives no bound
    /// on when it delivers, so on a busy Mac this fails however long it waits; until #756 the
    /// "about a second" of #264 is checked on Linux's inotify only.
    /// `cargo test -p charter-app planewatch -- --ignored` runs it.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "FSEvents has no delivery bound on a busy Mac (#577, #756)"]
    fn on_fsevents_a_todo_file_removed_under_a_watched_plane_is_told_within_the_second() {
        a_removed_todo_is_told_within_the_second_on::<notify::RecommendedWatcher>();
    }

    fn a_removed_todo_is_told_within_the_second_on<W: notify::Watcher + Send + 'static>() {
        let plane = plane_with_todos(&["m8-1", "m8-2"]);
        let root = plane.path().canonicalize().expect("canonical");
        let (_watch, told) = watching_on::<W>(&root);

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
        // The batch that told it also followed `beta/` and `beta/todos/`, before telling; a
        // directory is watched from its own contents onward, so nothing else is pending here.

        std::fs::write(root.join("workspaces/beta/todos/new.md"), "# new\n").expect("a todo");
        told.recv_timeout(PATIENCE)
            .expect("told about a todo in the new workspace");
    }

    /// `git` in `dir`, through charter's hardened runner, never signing.
    fn git(dir: &Path, args: &[&str]) {
        let argv: Vec<&str> = ["-c", "commit.gpgsign=false"]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        let done = charter_core::worktree::git::run(dir, &argv, charter_core::worktree::git::READ)
            .expect("git runs in a test");
        assert!(done.ok(), "git {args:?}: {done:?}");
    }

    #[test]
    fn a_change_the_watch_tells_is_in_the_next_shared_standing() {
        // FD-11: the window reads the plane again on `plane-changed`, and what it reads is the
        // standing every poller shares, so the watch makes it current before it tells.
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        git(&root, &["init", "-q", "-b", "main", "."]);
        git(&root, &["config", "user.email", "t@example.invalid"]);
        git(&root, &["config", "user.name", "t"]);
        git(&root, &["commit", "-q", "--allow-empty", "-m", "one"]);
        assert!(
            charter_core::planegit::shared_standing(&root)
                .changed
                .is_empty()
        );
        let (_watch, told) = watching(&root);

        std::fs::write(root.join("workspaces/alpha/todos/new.md"), "# new\n").expect("a todo");
        told.recv_timeout(PATIENCE).expect("told about the todo");

        assert_eq!(
            charter_core::planegit::shared_standing(&root).changed,
            vec!["workspaces/alpha/todos/new.md".to_owned()]
        );
    }

    #[test]
    fn a_plane_nobody_touches_is_never_told_about_its_own_making() {
        // What the tests used to sleep for: FSEvents hands a stream the writes that made the
        // plane BEFORE the stream began, as late as its daemon gets to them. A watch that reports
        // only what moved after it started is one a test can act against at once.
        a_plane_nobody_touches_is_quiet_on::<Source>();
    }

    /// The same on the watcher the app runs on macOS, where it does NOT hold: FSEvents told
    /// a stream about the plane's making seconds after it began. Ignored until #756 decides
    /// what the app does about slow file events. `-- --ignored` runs it.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "FSEvents replays the plane's making late on a busy Mac (#577, #756)"]
    fn on_fsevents_a_plane_nobody_touches_is_never_told_about_its_own_making() {
        a_plane_nobody_touches_is_quiet_on::<notify::RecommendedWatcher>();
    }

    fn a_plane_nobody_touches_is_quiet_on<W: notify::Watcher + Send + 'static>() {
        let plane = plane_with_todos(&["m8-1", "m8-2"]);
        let root = plane.path().canonicalize().expect("canonical");
        let (_watch, told) = watching_on::<W>(&root);
        assert!(told.recv_timeout(QUIET_FOR * 8).is_err());
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

    /// Waits for a batch naming `path`, and returns what it said that path is.
    fn told_about(told: &mpsc::Receiver<(PlaneId, What)>, path: &str) -> Change {
        let until = Instant::now() + PATIENCE;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            let (_, what) = told.recv_timeout(left).expect("told the plane changed");
            // A batch the watcher could not place is told as `None`, and is not this test's
            // business: the change it waits for comes in a batch of its own or the next one.
            let Some(changes) = what else {
                continue;
            };
            if let Some(found) = changes.into_iter().find(|change| change.path == path) {
                return found;
            }
        }
        panic!("never told about {path}");
    }

    fn event(kind: EventKind, paths: &[&Path]) -> notify_debouncer_full::DebouncedEvent {
        let event = paths.iter().fold(notify::Event::new(kind), |event, path| {
            event.add_path(path.to_path_buf())
        });
        notify_debouncer_full::DebouncedEvent::new(event, Instant::now())
    }

    #[test]
    fn a_batch_is_told_as_unknown_when_an_event_names_no_path_or_asks_for_a_rescan() {
        use notify::event::{CreateKind, Flag};
        let root = Path::new("/home/dev/plane");
        let todo = root.join("workspaces/alpha/todos/a.md");
        let placed = event(EventKind::Create(CreateKind::File), &[&todo]);
        let named = what_changed(root, None, &[&placed]).expect("placed");
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].kind, Kind::Todos);

        let pathless = event(EventKind::Any, &[]);
        assert_eq!(what_changed(root, None, &[&placed, &pathless]), None);

        let mut rescan = event(EventKind::Other, &[&todo]);
        rescan.event = rescan.event.set_flag(Flag::Rescan);
        assert_eq!(what_changed(root, None, &[&placed, &rescan]), None);
    }

    #[test]
    fn a_root_opened_through_a_link_places_paths_the_disk_spells_its_own_way() {
        // macOS hands FSEvents paths as `/private/var/...` for a root opened as `/var/...`.
        use notify::event::CreateKind;
        let plane = plane_with_todos(&[]);
        let real = plane.path().canonicalize().expect("canonical");
        let links = tempfile::tempdir().expect("a place for the link");
        let link = links.path().join("plane");
        std::os::unix::fs::symlink(&real, &link).expect("a link to the plane");
        let todo = real.join("workspaces/alpha/todos/a.md");
        let written = event(EventKind::Create(CreateKind::File), &[&todo]);

        assert_eq!(what_changed(&link, None, &[&written]), None);
        let placed = what_changed(&link, Some(&real), &[&written]).expect("placed");
        assert_eq!(placed[0].path, "workspaces/alpha/todos/a.md");
        assert_eq!(placed[0].workspace.as_deref(), Some("alpha"));
    }

    #[test]
    fn a_memory_an_agent_writes_in_a_workspace_is_told_as_that_workspaces_memory() {
        // The stale-data gap: `memory/` was not watched, so a memory an agent saved did not
        // reach the Memory panel until some other change happened to arrive.
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        std::fs::create_dir_all(root.join("workspaces/alpha/memory")).expect("a journal");
        let (_watch, told) = telling(&root);

        std::fs::write(root.join("workspaces/alpha/memory/note.md"), "# note\n").expect("a memory");

        let change = told_about(&told, "workspaces/alpha/memory/note.md");
        assert_eq!(change.kind, Kind::Memory);
        assert_eq!(change.workspace.as_deref(), Some("alpha"));
    }

    #[test]
    fn a_session_record_and_a_persona_memory_are_told_by_what_they_are() {
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        std::fs::create_dir_all(root.join("sessions")).expect("the root's records");
        std::fs::create_dir_all(root.join("workspaces/alpha/sessions")).expect("records");
        std::fs::create_dir_all(root.join("personas/steward/memory")).expect("a persona");
        let (_watch, told) = telling(&root);

        std::fs::write(root.join("sessions/r.md"), "# r\n").expect("a root record");
        let change = told_about(&told, "sessions/r.md");
        assert_eq!(change.kind, Kind::Sessions);
        assert_eq!(change.workspace, None);

        std::fs::write(root.join("workspaces/alpha/sessions/r.md"), "# r\n").expect("a record");
        let change = told_about(&told, "workspaces/alpha/sessions/r.md");
        assert_eq!(change.kind, Kind::Sessions);
        assert_eq!(change.workspace.as_deref(), Some("alpha"));

        std::fs::write(root.join("personas/steward/memory/m.md"), "# m\n").expect("a memory");
        let change = told_about(&told, "personas/steward/memory/m.md");
        assert_eq!(change.kind, Kind::Memory);
        assert_eq!(change.persona.as_deref(), Some("steward"));
    }

    #[test]
    fn a_todo_closed_is_told_as_a_todo_of_its_workspace() {
        let plane = plane_with_todos(&["m8-1"]);
        let root = plane.path().canonicalize().expect("canonical");
        let (_watch, told) = telling(&root);

        std::fs::remove_file(root.join("workspaces/alpha/todos/m8-1.md")).expect("closed");

        let change = told_about(&told, "workspaces/alpha/todos/m8-1.md");
        assert_eq!(change.kind, Kind::Todos);
        assert_eq!(change.workspace.as_deref(), Some("alpha"));
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
        std::fs::create_dir_all(root.join("workspaces/alpha/memory/archive")).expect("a journal");
        std::fs::create_dir_all(root.join("workspaces/alpha/sessions")).expect("records");
        std::fs::create_dir_all(root.join("sessions")).expect("the root's records");

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
                "personas/steward/memory",
                "sessions",
                "workspaces",
                "workspaces/alpha",
                "workspaces/alpha/memory",
                "workspaces/alpha/sessions",
                "workspaces/alpha/todos"
            ]
        );
    }

    #[test]
    fn the_window_is_told_which_answers_a_change_concerns_and_every_one_when_it_is_not_known() {
        let plane = id(Path::new("/home/dev/plane"));
        let memory = planechange::classify(
            Path::new("/home/dev/plane"),
            Path::new("/home/dev/plane/workspaces/alpha/memory/m.md"),
        )
        .expect("placed");
        let told = PlaneChanged::of(plane.clone(), Some(vec![memory]));
        assert_eq!(
            told.answers,
            Some(vec![
                PlaneAnswer::Panels {
                    workspace: Some("alpha".to_owned())
                },
                PlaneAnswer::Views
            ])
        );
        assert_eq!(
            serde_json::to_value(&told.answers).expect("serialisable"),
            serde_json::json!([{"answer": "panels", "workspace": "alpha"}, {"answer": "views"}])
        );

        let unknown = PlaneChanged::of(plane, None);
        assert_eq!(unknown.changes, None);
        assert_eq!(unknown.answers, None);
    }
}
