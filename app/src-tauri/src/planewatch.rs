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
//!   it (charter#369, [`purlis_core::instructions`]).
//!
//! Not the plane recursively, because a workspace holds its clones: a recursive inotify watch
//! would put one watch on every directory of every clone's `node_modules/` and `target/`, and
//! run the machine out of watches. (On macOS FSEvents streams the subtree regardless and notify
//! drops what is not a direct child in-process, so there the narrow set is about what is
//! REPORTED, not what is watched.) The set is re-read after every batch, so a workspace made
//! in a terminal is watched from then on.
//!
//! # One event per burst
//!
//! The platform's events are folded into bursts ([`crate::watchset::bursts`]) — `git pull`
//! landing ten todos, an editor's write-then-rename — and a burst is one `plane-changed` carrying the plane
//! **and what changed in it**: each path, and what it is part of
//! ([`purlis_core::planechange`]) — a todo of `alpha`, a memory of `steward`, the root's
//! session records. A panel reads again only on a change its answer is made of (FD-10), so a
//! memory an agent saves no longer makes the sidebar list every workspace's todos once more.
//!
//! A burst this cannot place — a path outside the plane, an event notify could not name a path
//! for, a rescan, more paths than a burst holds — is told as `changes: null`, "anything may have moved", and every reader
//! reads again, as each one did before there were kinds.
//!
//! **An access is not a change.** notify's inotify backend watches `IN_OPEN`, and reading a
//! todo opens it — so a window that re-read on every event would re-read because it re-read,
//! forever. [`crate::watchset::matters`] is where that loop is cut.
//!
//! **A file made and removed inside one burst is still told** (#1139). The debouncer this used
//! to fold through took a removal that arrived while the creation was still queued as the file
//! never having been there, and told neither — but a read the window made on another change
//! may already have drawn that file.
//!
//! # When the events are late (#756)
//!
//! macOS's file-event service serves the whole machine, and on a busy Mac it was measured
//! handing a stream a change 4 to 15 seconds late, and sometimes not within four minutes, with
//! no flag saying it had dropped anything (#577). The sidebar's model and the panels answer
//! from what the watch told them (FD-10b), so a change the events never tell is one the window
//! never draws. So the watch also **looks for itself** ([`Windows`]):
//!
//! - **while a window is shown**, every [`SWEEP_EVERY`], it reads the entries of the folders it
//!   watches (one `read_dir` and one `lstat` each, no file is opened) and tells what differs
//!   from its last look and no event has told — after a [`GRACE`] for the event to arrive;
//! - **when a window is focused**, it looks at once, so coming back to the window after a
//!   terminal wrote the project shows the terminal's change. A storm of focus changes is one
//!   look per [`QUIET_FOR`];
//! - **while every window is hidden or minimised** it does not look at all (SC-18, #1392); the
//!   events it hears meanwhile are kept, and the first look after a focus compares against the
//!   look before the windows went;
//! - each change the look had to tell is counted for the doctor's `file events` row
//!   ([`Windows::missed`]).
//!
//! A look tells the changed entries as a burst of their own, placed as the events' are, so the
//! readers that hold the changed answers read again and nothing else does. A folder entry is
//! compared by its being there, not its time: a clone's own writes move its folder's time, and
//! no non-recursive watch tells those.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, LazyLock, Mutex, OnceLock, PoisonError, Weak, mpsc};
use std::time::{Duration, Instant, SystemTime};

use purlis_core::planechange::{self, Answer, Change, Kind};
use purlis_core::workspaces::Plane;

use crate::planes::PlaneId;
use crate::watchset::Burst;

/// The event the window is sent.
pub(crate) const CHANGED: &str = "plane-changed";

/// What `plane-changed` carries: which plane moved, and what moved in it. Every window filters
/// on the plane, as it filters `chat-moved`, because the app holds several planes and emits on
/// the app.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct PlaneChanged {
    pub plane: PlaneId,
    /// Each changed path and what it is part of, or `null` when what changed is not known —
    /// a batch this could not place, or auto-save having moved the tree — and every reader
    /// reads again. Auto-save that left the tree as it was says so as one change of kind
    /// `git`, with no path.
    pub changes: Option<Vec<PlaneChange>>,
    /// The answers these changes concern, each once, or `null` — every answer — when what
    /// changed is not known. A reader names the answer it holds and reads again only when it
    /// is here: which answer a change concerns is the core's question
    /// ([`purlis_core::planechange::answers`]), never the window's.
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

/// One answer the window reads from the plane ([`purlis_core::planechange::Answer`],
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
    /// `chats_plane_updated`: the instructions a chat read at its start.
    Instructions,
    /// `curation_offers`: the curation actions offered on each subject.
    Curations,
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
            Answer::Instructions => Self::Instructions,
            Answer::Curations => Self::Curations,
            Answer::Settings => Self::Settings,
            Answer::Git => Self::Git,
            Answer::Views => Self::Views,
        }
    }
}

/// What one changed path is part of, as the window's readers divide the plane
/// ([`purlis_core::planechange::Kind`], which this mirrors for the bindings).
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
    Chats,
}

/// One changed path ([`purlis_core::planechange::Change`], mirrored for the bindings).
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
                Kind::Chats => ChangeKind::Chats,
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

/// The most changed paths one burst holds. Past it the burst is told as "anything may have
/// moved": a flood costs a fixed amount of memory, and every reader reads again.
const MOST_PATHS: usize = 256;

/// The watcher and the directories it is watching, behind one lock: the set is re-read on the
/// watch's own thread after every burst.
struct Inner<W: notify::Watcher> {
    watcher: Option<W>,
    watched: HashSet<PathBuf>,
    /// The folders the platform would not watch at the last follow, which are still there.
    unwatched: HashSet<PathBuf>,
    /// Whether any folder went unwatched since [`Watch::standing`] last said the watch was
    /// whole.
    lapsed: bool,
    rewatch: Rewatch,
    /// What the events told since the look before last ([`Heard`]).
    heard: Heard,
}

/// The paths the events named, kept for the look ([`Windows`]) to leave alone what they told.
#[derive(Debug, Default)]
struct Heard {
    paths: HashSet<PathBuf>,
    /// A burst was everything: whatever the look finds, the events said.
    everything: bool,
}

impl Heard {
    /// The most paths kept. Past it, as a burst past its cap is, the events said everything.
    const MOST: usize = MOST_PATHS * 4;

    fn add(&mut self, burst: &Burst) {
        if self.everything {
            return;
        }
        if burst.everything || self.paths.len() + burst.paths.len() > Self::MOST {
            self.everything = true;
            self.paths = HashSet::new();
            return;
        }
        self.paths.extend(burst.paths.iter().cloned());
    }

    /// Whether an event named `path`, or the folder it is in: FSEvents may name the folder.
    fn told(&self, path: &Path) -> bool {
        self.everything
            || self.paths.contains(path)
            || path.parent().is_some_and(|dir| self.paths.contains(dir))
    }
}

/// How far a plane's watch can be trusted to tell everything the panels and the sidebar read
/// ([`Watch::standing`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Every folder is watched, and has been since this was last asked.
    Whole,
    /// Every folder is watched now, but some went unwatched since this was last asked: what
    /// was held from the watch's word over that gap has to be read again.
    WholeAgain,
    /// Some folder the platform would not watch: read the disk, not what was held.
    Partial,
}

/// The watcher the app's plane watch runs on: the platform's own (FSEvents, inotify). In this
/// crate's tests on macOS, notify's poller, looking every 50 ms: FSEvents gives no bound on
/// when it delivers (#577, #756), and a test of a held plane waits on it.
#[cfg(not(all(test, target_os = "macos")))]
pub type Platform = notify::RecommendedWatcher;
#[cfg(all(test, target_os = "macos"))]
pub type Platform = notify::PollWatcher;

/// How [`Platform`] is configured.
fn platform_config() -> notify::Config {
    let config = notify::Config::default();
    #[cfg(all(test, target_os = "macos"))]
    let config = config.with_poll_interval(Duration::from_millis(50));
    config
}

/// The least time between two drops of every watch ([`Rewatch`]).
const REWATCH_EVERY: Duration = Duration::from_secs(1);

/// When a burst that was everything drops every watch and watches the set again.
///
/// **Only on inotify**, whose watch goes with its directory: a directory removed and made
/// again inside a burst too big to say which paths it held would otherwise never be watched
/// again. FSEvents and the others watch a path, not a directory, and dropping every watch there
/// restarts the stream once per folder for nothing, so they only follow the set again.
/// **And at most once a [`REWATCH_EVERY`]**: a drop owed sooner is made on the first burst
/// after it.
#[derive(Debug, Default)]
struct Rewatch {
    owed: bool,
    last: Option<Instant>,
}

impl Rewatch {
    /// Whether to drop every watch now, at `at`, after a burst on `kind` that was or was not
    /// `everything`.
    fn now(&mut self, kind: notify::WatcherKind, everything: bool, at: Instant) -> bool {
        if everything && kind == notify::WatcherKind::Inotify {
            self.owed = true;
        }
        if !self.owed
            || self
                .last
                .is_some_and(|last| at.saturating_duration_since(last) < REWATCH_EVERY)
        {
            return false;
        }
        self.owed = false;
        self.last = Some(at);
        true
    }
}

/// How often a watch looks for itself while a window is shown (#756). Five seconds keeps a
/// missed change within the "few seconds" the issue asks for, and costs a sidebar-sized
/// project well under a millisecond a look — a few hundred `lstat`s, measured in the PR — so
/// the look is a rounding error against SC-18's idle budget (#1392). Nothing looks while every
/// window is hidden.
pub(crate) const SWEEP_EVERY: Duration = Duration::from_secs(5);

/// How long a difference the look found waits for its event before the look tells it itself.
/// FSEvents on a quiet Mac delivers well inside it; a change it found sooner than its event
/// would otherwise be counted as missed.
const GRACE: Duration = Duration::from_secs(1);

/// How long a change the look had to tell keeps the doctor's `file events` row a warning.
pub(crate) const LATELY: Duration = Duration::from_secs(600);

/// The app's windows as the plane watches' looks follow them (#756): whether one is shown, and
/// each time one was focused. One for the app ([`windows`]); a test makes its own.
pub struct Windows {
    state: Mutex<Shown>,
    wake: Condvar,
    /// Whether any window is shown now, asked from a look's own thread. Until it is set, every
    /// window counts as shown.
    shown: OnceLock<Box<dyn Fn() -> bool + Send + Sync>>,
    /// Each watched root: how many watches watch it, and the changes their looks had to tell.
    /// Counted, so a project's watch started again before the old one is dropped is not
    /// forgotten when the old one goes.
    missed: Mutex<HashMap<PathBuf, (usize, Missed)>>,
}

#[derive(Debug, Default)]
struct Shown {
    /// Every window was found hidden or minimised at the last look: nothing looks until one is
    /// focused.
    hidden: bool,
    /// How many times a window was focused.
    focused: u64,
}

/// What a watch's looks had to tell because no event did, since the watch started.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Missed {
    /// How many changes.
    pub count: u64,
    /// When the last was told.
    pub last: Option<Instant>,
}

/// Why a look's thread woke.
enum Wake {
    /// Its watch is gone.
    Gone,
    /// A window was focused.
    Focused,
    /// Its time to look came.
    Look,
}

static WINDOWS: LazyLock<Arc<Windows>> = LazyLock::new(Windows::new);

/// The app's [`Windows`], which every plane the app opens is watched with.
pub fn windows() -> &'static Arc<Windows> {
    &WINDOWS
}

/// A window's focus changed (`lib.rs`'s `WindowEvent::Focused` arm, #756): the first time, the
/// app's [`Windows`] learn how to ask whether any window is shown — visible and not minimised —
/// and each time a window gains focus, every plane watch looks now and goes on looking.
pub fn window_focused(app: &tauri::AppHandle, focused: bool) {
    let watches = windows();
    if watches.shown.get().is_none() {
        let app = app.clone();
        watches.shown_when(move || {
            use tauri::Manager as _;
            app.webview_windows().values().any(|window| {
                window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false)
            })
        });
    }
    if focused {
        watches.focused();
    }
}

impl Windows {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(Shown::default()),
            wake: Condvar::new(),
            shown: OnceLock::new(),
            missed: Mutex::new(HashMap::new()),
        })
    }

    /// How a look asks whether any window is shown. The first one given is kept.
    pub fn shown_when(&self, shown: impl Fn() -> bool + Send + Sync + 'static) {
        let _ = self.shown.set(Box::new(shown));
    }

    /// A window was focused: every watch looks now, and goes on looking.
    pub fn focused(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.hidden = false;
        state.focused += 1;
        drop(state);
        self.wake.notify_all();
    }

    /// What the looks of the watch on `root` had to tell, or `None` when nothing watches it.
    pub fn missed(&self, root: &Path) -> Option<Missed> {
        self.missed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(root)
            .map(|(_, missed)| *missed)
    }

    fn any_shown(&self) -> bool {
        self.shown.get().is_none_or(|shown| shown())
    }

    fn hide(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .hidden = true;
    }

    fn focused_count(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .focused
    }

    /// Waits for a focus past `seen`, for `every` to pass while a window is shown, or for
    /// `alive` to say no. `alive` is asked under this lock, and a watch's drop takes it before
    /// it wakes the looks, so a drop between the asking and the waiting is never slept through.
    fn wait(&self, seen: u64, every: Duration, alive: &dyn Fn() -> bool) -> Wake {
        let until = Instant::now() + every;
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if !alive() {
                return Wake::Gone;
            }
            if state.focused != seen {
                return Wake::Focused;
            }
            if state.hidden {
                state = self
                    .wake
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
                continue;
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Wake::Look;
            }
            state = self
                .wake
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Wakes every look's thread to see whether its watch is still there.
    fn wake_all(&self) {
        drop(self.state.lock().unwrap_or_else(PoisonError::into_inner));
        self.wake.notify_all();
    }

    fn watching(&self, root: &Path) {
        self.missed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(root.to_path_buf())
            .or_default()
            .0 += 1;
    }

    fn not_watching(&self, root: &Path) {
        let mut missed = self.missed.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((watches, _)) = missed.get_mut(root) {
            *watches = watches.saturating_sub(1);
            if *watches == 0 {
                missed.remove(root);
            }
        }
    }

    fn told_missed(&self, root: &Path, count: usize, at: Instant) {
        if let Some(missed) = self
            .missed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(root)
            .map(|(_, missed)| missed)
        {
            missed.count += count as u64;
            missed.last = Some(at);
        }
    }
}

/// One entry of a watched folder, as a look compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stamp {
    /// A folder: compared by its being there (see the module's header).
    Folder,
    /// Anything else, by its time and length, as `lstat` gives them.
    Other {
        modified: Option<SystemTime>,
        len: u64,
    },
}

/// What one look saw: each watched folder it could read, and its entries.
type Look = HashMap<PathBuf, HashMap<OsString, Stamp>>;

/// The entries of each of `dirs`. A folder that cannot be read is left out, and compared with
/// nothing: its going is seen in the folder it was in.
fn look<'a>(dirs: impl IntoIterator<Item = &'a PathBuf>) -> Look {
    dirs.into_iter()
        .filter_map(|dir| {
            let entries = std::fs::read_dir(dir).ok()?;
            let entries = entries
                .flatten()
                .filter_map(|entry| {
                    let found = std::fs::symlink_metadata(entry.path()).ok()?;
                    let stamp = if found.file_type().is_dir() {
                        Stamp::Folder
                    } else {
                        Stamp::Other {
                            modified: found.modified().ok(),
                            len: found.len(),
                        }
                    };
                    Some((entry.file_name(), stamp))
                })
                .collect();
            Some((dir.clone(), entries))
        })
        .collect()
}

/// What moved between two looks, in the folders both read: the paths that differ, and among
/// them the ones that went.
fn differ(before: &Look, now: &Look) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
    let mut changed = HashSet::new();
    let mut removed = HashSet::new();
    for (dir, entries) in now {
        let Some(was) = before.get(dir) else {
            continue;
        };
        for (name, stamp) in entries {
            if was.get(name) != Some(stamp) {
                changed.insert(dir.join(name));
            }
        }
        for name in was.keys().filter(|name| !entries.contains_key(*name)) {
            let path = dir.join(name);
            removed.insert(path.clone());
            changed.insert(path);
        }
    }
    (changed, removed)
}

/// One plane's watch. Dropping it stops it.
///
/// `W` is where the changes come from: the platform's own watcher (FSEvents, inotify) in the
/// app. The tests on macOS choose notify's poller instead, because FSEvents gives no bound on
/// when it delivers (#577): its one daemon serves the whole machine, and on a Mac busy writing
/// build trees it was measured handing a stream a change 4 to 15 seconds late, and a stream
/// nothing at all for minutes. What this module decides — fold a burst, follow a new workspace,
/// fall silent when dropped — is the same whichever watcher feeds it.
pub struct Watch<W: notify::Watcher = Platform> {
    inner: Arc<Mutex<Inner<W>>>,
    /// What its looks follow ([`Windows`]), and the root it is counted under there.
    windows: Arc<Windows>,
    root: PathBuf,
}

impl Watch {
    /// Starts watching `root` and tells `changed` about `plane` whenever it moves, looking for
    /// itself as the app's windows are shown and focused ([`windows`]).
    pub fn start(plane: PlaneId, root: &Path, changed: Changed) -> notify::Result<Self> {
        Self::start_with(
            plane,
            root,
            changed,
            platform_config(),
            Arc::clone(windows()),
            SWEEP_EVERY,
        )
    }
}

impl<W: notify::Watcher> Watch<W> {
    /// Whether this watch tells everything the panels and the sidebar read, and whether it
    /// has since it was last asked. `WholeAgain` is said once: asking again says `Whole`.
    pub fn standing(&self) -> Standing {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if !inner.unwatched.is_empty() {
            Standing::Partial
        } else if std::mem::take(&mut inner.lapsed) {
            Standing::WholeAgain
        } else {
            Standing::Whole
        }
    }
}

impl<W: notify::Watcher + Send + 'static> Watch<W> {
    /// [`Watch::start`] on a watcher of the caller's choosing, configured by `config`, its looks
    /// following `windows` every `every` while one is shown.
    fn start_with(
        plane: PlaneId,
        root: &Path,
        changed: Changed,
        config: notify::Config,
        windows: Arc<Windows>,
        every: Duration,
    ) -> notify::Result<Self> {
        let (sent, events) = mpsc::channel();
        let watcher = W::new(crate::watchset::sender(sent), config)?;
        let inner = Arc::new(Mutex::new(Inner {
            watcher: Some(watcher),
            watched: HashSet::new(),
            unwatched: HashSet::new(),
            lapsed: false,
            rewatch: Rewatch::default(),
            heard: Heard::default(),
        }));
        let first = {
            let mut inner = inner.lock().unwrap_or_else(PoisonError::into_inner);
            inner.follow(root);
            // The root is where a workspace or a persona arriving is heard: without it the
            // watch could never become whole, so it is no watch.
            if inner.unwatched.contains(root) {
                return Err(notify::Error::generic(&format!(
                    "the platform would not watch {}",
                    root.display()
                )));
            }
            // The first look is taken now, so a change the events miss from here on is one
            // the next look finds.
            look(&inner.watched)
        };
        let handle: Weak<Mutex<Inner<W>>> = Arc::downgrade(&inner);
        let at = root.to_path_buf();
        // The platform reports paths as the disk spells them, which a root opened through a
        // link (`/var` for `/private/var` on macOS) is not.
        let spelled = root.canonicalize().ok();
        let respell = spelled.clone();
        let tell = {
            let handle = handle.clone();
            let at = at.clone();
            move |burst: Burst| {
                let what = what_changed(&at, spelled.as_deref(), &burst);
                // The set first, so a workspace made in this burst is watched before the window
                // reads it, and a change inside it straight after is not missed.
                //
                // **And nothing at all once the watch is dropped.** Dropping it ends this
                // thread, but a burst already gathered still arrives here, and a plane that
                // has been closed must not be reported.
                {
                    let Some(inner) = handle.upgrade() else {
                        return;
                    };
                    let mut inner = inner.lock().unwrap_or_else(PoisonError::into_inner);
                    if inner.watcher.is_none() {
                        return;
                    }
                    inner.heard.add(&burst);
                    // What went is not known, so on inotify nothing watched can be trusted to
                    // still be.
                    if inner
                        .rewatch
                        .now(W::kind(), burst.everything, Instant::now())
                    {
                        inner.forget_all();
                    }
                    for path in &burst.removed {
                        inner.forget(path);
                    }
                    inner.follow(&at);
                }
                // Before the window hears it, so the reads it makes on `plane-changed` are of
                // the plane as it is now, not the shared standings from before (FD-11).
                purlis_core::planegit::touch(&at);
                changed(plane.clone(), what);
            }
        };
        let tell: Arc<dyn Fn(Burst) + Send + Sync> = Arc::new(tell);
        {
            let tell = Arc::clone(&tell);
            std::thread::Builder::new()
                .name("charter-plane-watch".into())
                .spawn(move || {
                    for burst in crate::watchset::bursts(events, QUIET_FOR, MOST_PATHS) {
                        tell(burst);
                    }
                })
                .map_err(notify::Error::io)?;
        }
        windows.watching(root);
        let looking = Looking {
            inner: handle,
            windows: Arc::clone(&windows),
            root: at,
            spelled: respell,
            every,
            tell,
        };
        let started = std::thread::Builder::new()
            .name("charter-plane-look".into())
            .spawn(move || looking.run(first));
        if let Err(why) = started {
            windows.not_watching(root);
            return Err(notify::Error::io(why));
        }
        Ok(Self {
            inner,
            windows,
            root: root.to_path_buf(),
        })
    }
}

/// A watch's own looks (#756, the module's header), on a thread of their own.
struct Looking<W: notify::Watcher> {
    inner: Weak<Mutex<Inner<W>>>,
    windows: Arc<Windows>,
    root: PathBuf,
    /// The root as the disk spells it, which is how the events name what they told.
    spelled: Option<PathBuf>,
    every: Duration,
    tell: Arc<dyn Fn(Burst) + Send + Sync>,
}

impl<W: notify::Watcher> Looking<W> {
    /// Looks until the watch is dropped, from `before`, the look taken as it started.
    fn run(self, mut before: Look) {
        let mut seen = self.windows.focused_count();
        // What the events told in the take before the last: an event that arrived after one
        // look but was taken before the next one's difference is still what told it.
        let mut told_before = Heard::default();
        loop {
            match self.windows.wait(seen, self.every, &|| self.alive()) {
                Wake::Gone => return,
                Wake::Focused => {
                    // A storm of focus changes is one look: the ones inside this wait are
                    // folded into it.
                    std::thread::sleep(QUIET_FOR);
                    seen = self.windows.focused_count();
                }
                Wake::Look => {
                    // Asked outside every lock: the answer may come from the window's thread.
                    if !self.windows.any_shown() {
                        self.windows.hide();
                        continue;
                    }
                }
            }
            let Some(watched) = self.watched() else {
                return;
            };
            let now = look(&watched);
            let (changed, removed) = differ(&before, &now);
            if !changed.is_empty() {
                std::thread::sleep(GRACE);
            }
            let Some(told) = self.heard() else {
                return;
            };
            let missed: HashSet<PathBuf> = changed
                .into_iter()
                .filter(|path| {
                    let spelled = self.respelled(path);
                    ![Some(path.as_path()), spelled.as_deref()]
                        .into_iter()
                        .flatten()
                        .any(|path| told.told(path) || told_before.told(path))
                })
                .collect();
            told_before = told;
            if !missed.is_empty() {
                self.windows
                    .told_missed(&self.root, missed.len(), Instant::now());
                let removed = removed.intersection(&missed).cloned().collect();
                (self.tell)(Burst {
                    paths: missed,
                    removed,
                    ..Burst::default()
                });
            }
            before = self.merged(now);
        }
    }

    /// `path`, under the root as the disk spells it, where that is another spelling.
    fn respelled(&self, path: &Path) -> Option<PathBuf> {
        let spelled = self
            .spelled
            .as_ref()
            .filter(|spelled| **spelled != self.root)?;
        Some(spelled.join(path.strip_prefix(&self.root).ok()?))
    }

    /// Whether the watch is still there.
    fn alive(&self) -> bool {
        self.inner.upgrade().is_some_and(|inner| {
            inner
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .watcher
                .is_some()
        })
    }

    /// The folders the watch watches now, or `None` once it is dropped.
    fn watched(&self) -> Option<HashSet<PathBuf>> {
        let inner = self.inner.upgrade()?;
        let inner = inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.watcher.as_ref()?;
        Some(inner.watched.clone())
    }

    /// What the events told since this was last asked, or `None` once the watch is dropped.
    fn heard(&self) -> Option<Heard> {
        let inner = self.inner.upgrade()?;
        let mut inner = inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner.watcher.as_ref()?;
        Some(std::mem::take(&mut inner.heard))
    }

    /// `now`, and a first look at each folder the watch took up since (a workspace this look
    /// told): what the next look compares against.
    fn merged(&self, mut now: Look) -> Look {
        let Some(watched) = self.watched() else {
            return now;
        };
        let new: Vec<PathBuf> = watched
            .into_iter()
            .filter(|dir| !now.contains_key(dir))
            .collect();
        now.extend(look(&new));
        now
    }
}

impl<W: notify::Watcher> Drop for Watch<W> {
    fn drop(&mut self) {
        // Taken out under the lock and dropped outside it, so the watch's thread, which takes
        // the same lock, never waits on a drop. An empty slot is also what tells a burst
        // already in flight that nobody is listening any more.
        let watcher = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .watcher
            .take();
        drop(watcher);
        self.windows.not_watching(&self.root);
        // And its looks stop, whether a window is shown or not.
        self.windows.wake_all();
    }
}

impl<W: notify::Watcher> Inner<W> {
    /// Watches what the plane has now, and stops watching what it no longer has.
    fn follow(&mut self, root: &Path) {
        if let Some(watcher) = self.watcher.as_mut() {
            self.unwatched = crate::watchset::follow(watcher, &mut self.watched, wanted(root));
            if !self.unwatched.is_empty() {
                self.lapsed = true;
            }
        }
    }

    fn forget(&mut self, path: &Path) {
        if let Some(watcher) = self.watcher.as_mut() {
            crate::watchset::forget(watcher, &mut self.watched, path);
        }
    }

    fn forget_all(&mut self) {
        for path in std::mem::take(&mut self.watched) {
            if let Some(watcher) = self.watcher.as_mut() {
                let _ = watcher.unwatch(&path);
            }
        }
    }
}

/// What a burst changed, each path placed by [`planechange`] against the root as it was given
/// or, failing that, as the disk spells it — or `None` when any one of them cannot be placed
/// or the burst is everything: an event named no path, notify asked for a rescan (it dropped
/// events it cannot name), or there were more paths than a burst holds.
fn what_changed(root: &Path, spelled: Option<&Path>, burst: &Burst) -> What {
    if burst.everything {
        return None;
    }
    let paths = || burst.paths.iter().map(PathBuf::as_path);
    planechange::of_batch(root, paths())
        .or_else(|| spelled.and_then(|spelled| planechange::of_batch(spelled, paths())))
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
        looking_on::<W>(root, &Windows::new(), NEVER)
    }

    /// Longer than any test: a watch whose looks a test does not ask about looks only when its
    /// own windows are focused, which nothing does.
    const NEVER: Duration = Duration::from_secs(3600);

    /// [`telling_on`], its looks following `windows` every `every`.
    fn looking_on<W: notify::Watcher + Send + 'static>(
        root: &Path,
        windows: &Arc<Windows>,
        every: Duration,
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
            Arc::clone(windows),
            every,
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
    /// `cargo test -p purlis-app planewatch -- --ignored` runs it.
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
        // On the platform the test plays (#1139): a busy machine spread ten writes past the
        // debouncer's quarter-second tick and the poller's look, and they were told three
        // times. A burst now runs a quarter of a second from its first event, so what decides
        // is how the watch folds what it is handed, and that is what is tested.
        use crate::watchset::raw::{Raw, raw};
        use notify::event::{CreateKind, EventKind};
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        let (_watch, told) = watching_on::<Raw>(&root);

        for n in 0..10 {
            raw(
                EventKind::Create(CreateKind::File),
                &root.join(format!("workspaces/alpha/todos/t{n}.md")),
            );
        }

        told.recv_timeout(PATIENCE).expect("told");
        assert!(
            told.recv_timeout(QUIET_FOR * 4).is_err(),
            "a burst of ten writes was told more than once"
        );
    }

    #[test]
    fn a_todo_made_and_removed_inside_one_burst_is_still_told() {
        // The window may have drawn the todo on a read some other change set off; a watch that
        // took the removal as cancelling the creation would never tell it to draw it gone.
        use crate::watchset::raw::{Raw, raw};
        use notify::event::{CreateKind, EventKind, RemoveKind};
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        let (_watch, told) = telling_on::<Raw>(&root);
        let todo = root.join("workspaces/alpha/todos/brief.md");

        raw(EventKind::Create(CreateKind::File), &todo);
        raw(EventKind::Remove(RemoveKind::File), &todo);

        let change = told_about(&told, "workspaces/alpha/todos/brief.md");
        assert_eq!(change.kind, Kind::Todos);
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

    #[test]
    fn a_folder_the_platform_will_not_watch_leaves_the_watch_partial_until_it_is_watched() {
        // A watch that dropped a folder's registration and went on as if it watched the whole
        // plane served panels that never moved again (the FD-10 review). The watch says it is
        // partial, so its readers read the disk; once it watches everything again it says so
        // once, so they rebuild what they held over the gap.
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        let todos = root.join("workspaces/alpha/todos");
        crate::watchset::refuse::refuse(&todos);
        let (watch, told) = watching(&root);
        assert_eq!(watch.standing(), Standing::Partial);
        assert_eq!(
            watch.standing(),
            Standing::Partial,
            "still partial when asked again"
        );

        crate::watchset::refuse::allow(&todos);
        std::fs::create_dir_all(root.join("workspaces/beta")).expect("a change it is told");
        told.recv_timeout(PATIENCE).expect("told about beta");

        assert_eq!(watch.standing(), Standing::WholeAgain);
        assert_eq!(watch.standing(), Standing::Whole);
    }

    #[test]
    fn a_root_the_platform_will_not_watch_is_no_watch_at_all() {
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        crate::watchset::refuse::refuse(&root);
        let started = Watch::<Source>::start_with(
            id(&root),
            &root,
            Arc::new(|_, _| {}),
            notify::Config::default().with_poll_interval(LOOK_EVERY),
            Windows::new(),
            NEVER,
        );
        assert!(
            started.is_err(),
            "a watch that cannot watch the root started"
        );
    }

    /// `git` in `dir`, through charter's hardened runner, never signing.
    fn git(dir: &Path, args: &[&str]) {
        let argv: Vec<&str> = ["-c", "commit.gpgsign=false"]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        let done = purlis_core::worktree::git::run(dir, &argv, purlis_core::worktree::git::READ)
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
            purlis_core::planegit::shared_standing(&root)
                .changed
                .is_empty()
        );
        let (_watch, told) = watching(&root);

        std::fs::write(root.join("workspaces/alpha/todos/new.md"), "# new\n").expect("a todo");
        told.recv_timeout(PATIENCE).expect("told about the todo");

        assert_eq!(
            purlis_core::planegit::shared_standing(&root).changed,
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
        use crate::watchset::matters;
        use notify::EventKind;
        use notify::event::{
            AccessKind, AccessMode, CreateKind, MetadataKind, ModifyKind, RemoveKind,
        };
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

    /// A burst of the changes the platform reported on `paths`.
    fn burst(paths: &[&Path]) -> Burst {
        Burst {
            paths: paths.iter().map(|path| path.to_path_buf()).collect(),
            ..Burst::default()
        }
    }

    #[test]
    fn every_watch_is_dropped_on_inotify_only_and_at_most_once_a_second() {
        use notify::WatcherKind::{Inotify, PollWatcher};
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);

        let mut elsewhere = Rewatch::default();
        assert!(!elsewhere.now(PollWatcher, true, at(0)));

        let mut inotify = Rewatch::default();
        assert!(
            !inotify.now(Inotify, false, at(0)),
            "nothing was everything"
        );
        assert!(inotify.now(Inotify, true, at(0)));
        assert!(!inotify.now(Inotify, true, at(300)), "too soon: owed");
        assert!(!inotify.now(Inotify, false, at(900)), "still too soon");
        assert!(inotify.now(Inotify, false, at(1000)), "the owed drop, made");
        assert!(!inotify.now(Inotify, false, at(3000)), "nothing owed");
    }

    #[test]
    fn a_burst_is_told_as_unknown_when_anything_may_have_moved() {
        let root = Path::new("/home/dev/plane");
        let todo = root.join("workspaces/alpha/todos/a.md");
        let named = what_changed(root, None, &burst(&[&todo])).expect("placed");
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].kind, Kind::Todos);

        let everything = Burst {
            everything: true,
            ..Burst::default()
        };
        assert_eq!(what_changed(root, None, &everything), None);
    }

    #[test]
    fn a_root_opened_through_a_link_places_paths_the_disk_spells_its_own_way() {
        // macOS hands FSEvents paths as `/private/var/...` for a root opened as `/var/...`.
        let plane = plane_with_todos(&[]);
        let real = plane.path().canonicalize().expect("canonical");
        let links = tempfile::tempdir().expect("a place for the link");
        let link = links.path().join("plane");
        std::os::unix::fs::symlink(&real, &link).expect("a link to the plane");
        let todo = real.join("workspaces/alpha/todos/a.md");
        let written = burst(&[&todo]);

        assert_eq!(what_changed(&link, None, &written), None);
        let placed = what_changed(&link, Some(&real), &written).expect("placed");
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

    // #756: the watch's own looks. Each test's watch is fed by `Raw`, which reports nothing the
    // test does not send, so a write on disk is one whose event never came: a stalled FSEvents,
    // played without one. A real FSEvents stall cannot be made on demand; nothing here runs on
    // FSEvents itself.

    /// How often the looks of a test that waits on them look.
    const SOON: Duration = Duration::from_millis(100);

    /// Long enough for a look that was going to tell to have told: past its [`GRACE`].
    const TOLD_BY_NOW: Duration = Duration::from_secs(3);

    #[test]
    fn a_write_no_event_told_is_told_when_a_window_is_focused() {
        use crate::watchset::raw::Raw;
        let plane = plane_with_todos(&["m8-1"]);
        let root = plane.path().canonicalize().expect("canonical");
        let windows = Windows::new();
        let (_watch, told) = looking_on::<Raw>(&root, &windows, NEVER);

        std::fs::write(
            root.join("workspaces/alpha/todos/m8-1.md"),
            "# m8-1, written in a terminal\n",
        )
        .expect("a write");
        assert!(
            told.recv_timeout(QUIET_FOR * 4).is_err(),
            "told with no event and no focus"
        );
        windows.focused();

        let change = told_about(&told, "workspaces/alpha/todos/m8-1.md");
        assert_eq!(change.kind, Kind::Todos);
        assert_eq!(change.workspace.as_deref(), Some("alpha"));
        assert_eq!(windows.missed(&root).map(|missed| missed.count), Some(1));
    }

    #[test]
    fn a_removal_no_event_told_is_told_by_the_next_look() {
        use crate::watchset::raw::Raw;
        let plane = plane_with_todos(&["m8-1", "m8-2"]);
        let root = plane.path().canonicalize().expect("canonical");
        let windows = Windows::new();
        let (_watch, told) = looking_on::<Raw>(&root, &windows, SOON);

        std::fs::remove_file(root.join("workspaces/alpha/todos/m8-1.md")).expect("closed");

        let change = told_about(&told, "workspaces/alpha/todos/m8-1.md");
        assert_eq!(change.kind, Kind::Todos);
        let missed = windows.missed(&root).expect("watched");
        assert_eq!(missed.count, 1);
        assert!(missed.last.is_some());
    }

    #[test]
    fn nothing_is_looked_at_while_every_window_is_hidden_and_a_focus_looks_at_once() {
        use crate::watchset::raw::Raw;
        let plane = plane_with_todos(&["m8-1"]);
        let root = plane.path().canonicalize().expect("canonical");
        let windows = Windows::new();
        windows.shown_when(|| false);
        let (_watch, told) = looking_on::<Raw>(&root, &windows, SOON);
        // The first look's turn finds every window hidden, and stops looking.
        std::thread::sleep(SOON * 3);

        std::fs::remove_file(root.join("workspaces/alpha/todos/m8-1.md")).expect("closed");
        assert!(
            told.recv_timeout(TOLD_BY_NOW).is_err(),
            "looked while every window was hidden"
        );
        assert_eq!(windows.missed(&root).map(|missed| missed.count), Some(0));

        windows.focused();
        told_about(&told, "workspaces/alpha/todos/m8-1.md");
    }

    #[test]
    fn a_change_the_events_told_is_not_told_again_by_a_look() {
        use crate::watchset::raw::{Raw, raw};
        use notify::event::{CreateKind, EventKind};
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        let windows = Windows::new();
        let (_watch, told) = looking_on::<Raw>(&root, &windows, SOON);
        let todo = root.join("workspaces/alpha/todos/brief.md");

        std::fs::write(&todo, "# brief\n").expect("a todo");
        raw(EventKind::Create(CreateKind::File), &todo);

        told_about(&told, "workspaces/alpha/todos/brief.md");
        assert!(
            told.recv_timeout(TOLD_BY_NOW).is_err(),
            "a look told what the events had told"
        );
        assert_eq!(windows.missed(&root).map(|missed| missed.count), Some(0));
    }

    #[test]
    fn a_storm_of_focus_changes_is_one_look() {
        use crate::watchset::raw::Raw;
        let plane = plane_with_todos(&["m8-1"]);
        let root = plane.path().canonicalize().expect("canonical");
        let windows = Windows::new();
        let (_watch, told) = looking_on::<Raw>(&root, &windows, NEVER);

        std::fs::remove_file(root.join("workspaces/alpha/todos/m8-1.md")).expect("closed");
        for _ in 0..20 {
            windows.focused();
        }

        told_about(&told, "workspaces/alpha/todos/m8-1.md");
        assert!(
            told.recv_timeout(TOLD_BY_NOW).is_err(),
            "told more than once"
        );
        assert_eq!(windows.missed(&root).map(|missed| missed.count), Some(1));
    }

    #[test]
    fn a_workspace_a_look_found_is_watched_and_looked_into_from_then_on() {
        use crate::watchset::raw::Raw;
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        let windows = Windows::new();
        let (_watch, told) = looking_on::<Raw>(&root, &windows, SOON);

        std::fs::create_dir_all(root.join("workspaces/beta/todos")).expect("a new workspace");
        told_about(&told, "workspaces/beta");
        assert!(
            crate::watchset::raw::watched(&root.join("workspaces/beta/todos")) > 0,
            "the new workspace's todos are not watched"
        );

        std::fs::write(root.join("workspaces/beta/todos/new.md"), "# new\n").expect("a todo");
        let change = told_about(&told, "workspaces/beta/todos/new.md");
        assert_eq!(change.workspace.as_deref(), Some("beta"));
    }

    #[test]
    fn a_dropped_watch_is_no_longer_counted_and_its_looks_tell_nothing() {
        use crate::watchset::raw::Raw;
        let plane = plane_with_todos(&["m8-1"]);
        let root = plane.path().canonicalize().expect("canonical");
        let windows = Windows::new();
        let (watch, told) = looking_on::<Raw>(&root, &windows, SOON);
        assert_eq!(windows.missed(&root), Some(Missed::default()));

        drop(watch);
        std::fs::remove_file(root.join("workspaces/alpha/todos/m8-1.md")).expect("closed");
        windows.focused();

        assert_eq!(windows.missed(&root), None);
        assert!(told.recv_timeout(TOLD_BY_NOW).is_err());
    }

    /// A project watched again before its old watch is dropped is still counted once the old
    /// one goes: the doctor's row does not say it is not watched.
    #[test]
    fn a_watch_started_again_is_counted_after_the_old_one_is_dropped() {
        use crate::watchset::raw::Raw;
        let plane = plane_with_todos(&["m8-1"]);
        let root = plane.path().canonicalize().expect("canonical");
        let windows = Windows::new();
        let (old, _) = looking_on::<Raw>(&root, &windows, NEVER);
        let (_new, _) = looking_on::<Raw>(&root, &windows, NEVER);

        drop(old);

        assert_eq!(windows.missed(&root), Some(Missed::default()));
    }

    #[test]
    fn a_look_compares_a_folder_by_its_being_there_and_a_file_by_its_time_and_length() {
        // A clone's own writes move its folder's time, and no non-recursive watch tells them.
        let plane = plane_with_todos(&["m8-1"]);
        let root = plane.path().canonicalize().expect("canonical");
        std::fs::create_dir_all(root.join("workspaces/alpha/svc")).expect("a clone");
        let dirs = [
            root.join("workspaces/alpha"),
            root.join("workspaces/alpha/todos"),
        ];
        let before = look(&dirs);

        std::fs::write(root.join("workspaces/alpha/svc/README.md"), "busy\n").expect("a write");
        std::fs::write(root.join("workspaces/alpha/todos/m8-1.md"), "# changed\n").expect("a todo");
        std::fs::write(root.join("workspaces/alpha/todos/m8-2.md"), "# new\n").expect("a todo");
        let (changed, removed) = differ(&before, &look(&dirs));

        let todos = root.join("workspaces/alpha/todos");
        assert_eq!(
            changed,
            HashSet::from([todos.join("m8-1.md"), todos.join("m8-2.md")])
        );
        assert!(removed.is_empty());

        let before = look(&dirs);
        std::fs::remove_file(todos.join("m8-2.md")).expect("closed");
        let (changed, removed) = differ(&before, &look(&dirs));
        assert_eq!(changed, HashSet::from([todos.join("m8-2.md")]));
        assert_eq!(removed, HashSet::from([todos.join("m8-2.md")]));
    }

    /// The cost of one look on a large project, for the PR (#756, SC-18). Not a check: a
    /// number. `cargo test -p purlis-app --lib planewatch::tests::what_a_look_costs -- --ignored
    /// --nocapture` prints it.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn what_a_look_costs() {
        let plane = plane_with_todos(&[]);
        let root = plane.path().canonicalize().expect("canonical");
        for ws in 0..30 {
            for store in ["todos", "memory", "sessions"] {
                let dir = root.join(format!("workspaces/w{ws}/{store}"));
                std::fs::create_dir_all(&dir).expect("a store");
                for n in 0..20 {
                    std::fs::write(dir.join(format!("{n}.md")), "# x\n").expect("a file");
                }
            }
        }
        let dirs: Vec<PathBuf> = wanted(&root).into_iter().collect();
        let entries: usize = look(&dirs).values().map(HashMap::len).sum();
        let rounds = 200;
        let started = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(look(&dirs));
        }
        println!(
            "one look: {} folders, {entries} entries, {:?}",
            dirs.len(),
            started.elapsed() / rounds
        );
    }
}
