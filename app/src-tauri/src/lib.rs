//! The app's Rust side: what the UI can ask the core to do, as commands generated into
//! TypeScript by `tauri-specta`, so no shape is written by hand on either side.

// First, so its macros are defined for everything below.
#[macro_use]
mod ipc_commands;

/// `tauri::generate_context!`, with its arguments, in the one place that allows it: Tauri's own
/// expansion prints with `eprintln!`, which `clippy.toml` refuses in this crate (#647). The
/// allow is for Tauri's code, not charter's. Above every module, so each of them can use it.
macro_rules! tauri_context {
    ($($arg:tt)*) => {{
        #[allow(clippy::disallowed_macros)]
        let context = tauri::generate_context!($($arg)*);
        context
    }};
}

mod about;
mod alerts;
mod asking;
mod autosave;
mod branchwatch;
mod changes;
mod chats;
mod clipath;
mod curation;
mod doctor;
mod extensions;
mod filewatch;
mod findfiles;
mod firstrun;
mod firsttask;
mod handoff;
mod harness_plugins;
mod heard;
mod hooks;
mod host;
// Called on Linux alone, where the session bus can be missing; its tests run everywhere.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod instance;
mod ipc;
mod killswitch;
mod lifecycle;
mod live;
mod memories;
mod navguard;
mod off_the_main_thread;
mod opener;
mod panels;
mod panics;
mod personas;
mod piecefiles;
mod pin;
mod planes;
mod planewatch;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod portal;
mod references;
mod sandboxing;
mod saving;
mod searchfiles;
mod sessions;
mod settings;
mod slowstart;
mod smartclose;
mod thismachine;
mod todos;
mod updates;
mod usage;
mod vaults;
mod views;
mod watchset;
mod windowprefs;
mod windows;
mod worklinks;
mod workspaces;
mod worktrees;

use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use charter_core::engine::Size;
use charter_core::harness::Harness;
use charter_core::reopen::{Chat, Fresh, Reopened};
use tauri::Emitter;
use tauri::Manager;
use tauri::ipc::Channel;
use tauri_plugin_notification::NotificationExt;
use tauri_specta::{Builder, collect_commands};

use hooks::Moved;
use lifecycle::Quitting;
use planes::{Launch, PlaneId, Planes, Restoring, Showing};

/// The command-line binary a hook runs, or none when the app cannot find one.
///
/// Its own directory first, because the app and the binary are built and shipped together —
/// the one on `PATH` may be an older install, or the Python charter, and a hook pointed at
/// either would be answering a different program's idea of these events. `CHARTER_BINARY`
/// overrides it, which is how a scenario test points the hooks at the binary it just built.
///
/// Beside the app, `purlis` first — the command line itself — and then `charter`, the alias
/// that runs it (RN-3), so a build that carries only the old name still arms its hooks.
///
/// It must EXIST: arming a hook at a path that is not there would put an error in the
/// harness's log on every single event, which is worse than the chats reading `unknown`.
pub(crate) fn charter_binary() -> Option<PathBuf> {
    let named = charter_core::envvar::var_os("PURLIS_BINARY").map(PathBuf::from);
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.to_owned()));
    let beside = charter_core::cliname::INSTALLED
        .iter()
        .filter_map(move |name| Some(dir.as_ref()?.join(name)));
    named.into_iter().chain(beside).find(|path| path.is_file())
}

/// The shims a shell tab finds first on its `PATH`, written under the app's data directory to
/// run `binary` (ADR 0062). None where they cannot be: a shell tab is then a plain shell, which
/// is what it was before there were any, and the reason is said unless it is the platform's.
fn shell_tab_shims<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    binary: &std::path::Path,
) -> Option<charter_core::shellguard::Shims> {
    let dir = app.path().app_data_dir().ok()?.join("shims");
    let shims = charter_core::shellguard::Shims::at(&dir);
    match shims.write(binary) {
        Ok(()) => Some(shims),
        Err(why) if why.kind() == std::io::ErrorKind::Unsupported => None,
        Err(why) => {
            tracing::warn!(
                "charter: no shell-tab shims at {} ({why}); a harness started in a shell tab \
                 will not be warned about",
                dir.display()
            );
            None
        }
    }
}

/// The git hooks a harness chat commits through (SQ-16), written under the app's data directory
/// to run `binary`. None where they cannot be: a chat's commits are then not scanned, and the
/// reason is said unless it is the platform's.
fn chat_git_hooks<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    binary: &std::path::Path,
) -> Option<charter_core::githooks::GitHooks> {
    let dir = app.path().app_data_dir().ok()?.join("git-hooks");
    let hooks = charter_core::githooks::GitHooks::at(&dir);
    match hooks.write(binary) {
        Ok(()) => Some(hooks),
        Err(why) if why.kind() == std::io::ErrorKind::Unsupported => None,
        Err(why) => {
            tracing::warn!(
                "charter: no git hooks at {} ({why}); a chat's commits will not be scanned \
                 for secrets",
                dir.display()
            );
            None
        }
    }
}

/// What the app ships that a chat is armed with, found once at launch.
///
/// A property of this build and not of a project, so every plane arms with the same one.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shipped {
    /// The `charter` every hook runs ([`charter_binary`]).
    pub binary: Option<PathBuf>,
    /// The Claude Code plugin a chat loads (`charter_core::plugin`), inside the bundle.
    pub plugin: Option<PathBuf>,
    /// The shims a shell tab finds first on its `PATH` (ADR 0062), written at launch under the
    /// app's data directory — none where they could not be written, or off unix.
    pub shims: Option<charter_core::shellguard::Shims>,
    /// The git hooks every harness chat commits through (SQ-16), written at launch under the
    /// app's data directory — none where they could not be written, or off unix.
    pub git_hooks: Option<charter_core::githooks::GitHooks>,
}

/// Re-run `charter plugin install` for each harness whose installed copy runs this app's
/// `charter` and is out of date (`charter_core::plugin_install::refresh` has the rule), on a
/// thread of its own. Said on standard error, as `charter plugin install` says it; a harness
/// with nothing to do says nothing.
fn refresh_installed_plugin(binary: PathBuf, plugin: PathBuf) {
    use charter_core::plugin_install as install;
    if !install::refreshes_on_its_own(charter_core::fence::FENCED, cfg!(debug_assertions)) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("charter-plugin-refresh".into())
        .spawn(move || {
            // Resolved, as the `charter` that installed the copy named itself: a hook's path
            // compared with an unresolved one would call every copy somebody else's.
            let binary = binary.canonicalize().unwrap_or(binary);
            let machine = match install::Machine::from_env(binary, Some(plugin)) {
                Ok(machine) => machine,
                Err(why) => {
                    tracing::warn!("charter: the installed plugin was not checked: {why}");
                    return;
                }
            };
            let outcomes = install::refresh(&machine);
            if !outcomes.is_empty() {
                let said = if install::failed(&outcomes) {
                    "could not bring the installed plugin fully up to date with this app; \
                     `charter plugin install` says why"
                } else {
                    "brought the installed plugin up to date with this app"
                };
                // One event, without the newline `render` ends on: the log adds its own.
                tracing::info!(
                    "charter: {said}\n{}",
                    install::render(&outcomes, false).trim_end()
                );
            }
        });
}

/// The host's event log, opened once for the process (FD-9). A machine with no data home, or
/// one whose data home is refused, runs without it and says why: every chat still works, and
/// only the record of its hook calls is missing.
fn events() -> Option<hooks::Events> {
    let opened = charter_core::eventlog::Recorder::open();
    let _ = EVENT_LOG.set(match &opened {
        Ok(recorder) => Ok(recorder.dir().to_path_buf()),
        Err(why) => Err(EventLogRefused {
            kind: why.kind(),
            why: why.to_string(),
        }),
    });
    match opened {
        Ok(recorder) => Some(std::sync::Arc::new(std::sync::Mutex::new(recorder))),
        Err(why) => {
            tracing::warn!("charter: no event log ({why}); hook calls are not recorded");
            None
        }
    }
}

/// What opening the event log answered: its directory, or why there is none. The doctor's
/// `event log` row reads it.
pub(crate) static EVENT_LOG: std::sync::OnceLock<Result<std::path::PathBuf, EventLogRefused>> =
    std::sync::OnceLock::new();

/// Why the event log was not opened, kept for the doctor's row: the kind picks the repair.
#[derive(Debug, Clone)]
pub(crate) struct EventLogRefused {
    pub kind: std::io::ErrorKind,
    pub why: String,
}

/// Where the bundled plugin is, or none when this build has none.
///
/// Tauri's resource directory: `Contents/Resources` in a macOS bundle, `/usr/lib/charter` in a
/// `.deb` and an AppImage, and the directory the executable is in for a development build.
/// It must hold the plugin's manifest: `--plugin-dir` pointed at a directory that is not one
/// would start every chat with an error.
fn bundled_plugin(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?.join(PLUGIN_DIR);
    dir.join(".claude-plugin")
        .join("plugin.json")
        .is_file()
        .then_some(dir)
}

/// The plugin's directory, in the repository beside `tauri.conf.json` and in the bundle's
/// resources alike.
pub(crate) const PLUGIN_DIR: &str = "plugin";

/// What the window is sent whenever a chat moves, and the one case that also interrupts.
///
/// The window is always told; the notification is the narrow part. It fires on the edge into
/// the needs-you queue and only when the operator is not already looking at that chat — the
/// window hidden, the window not focused, or a different chat in front. At fifty sessions a
/// popup about the chat already on screen is noise, and noise is how a queue stops being read.
///
/// One wait cannot notify twice: the board answers "something changed" only on a change, so a
/// `Stop` that lands on a chat already waiting from a `Notification` moves nothing and says
/// nothing.
fn told(app: &tauri::AppHandle, moved: Moved) {
    windows::emit_for_plane(app, &moved.plane.clone(), "chat-moved", &moved);
    if moved.needs_you && !already_looking_at(app, &moved) {
        let name = chat_called(app, &moved).unwrap_or_else(|| format!("chat {}", moved.session));
        // Best effort, always. A desktop that refuses notifications, or an operator who
        // turned them off, is not a reason for anything else here to stop working.
        let _ = app
            .notification()
            .builder()
            .title(name)
            .body("needs you")
            .show();
    }
}

/// What a chat that moved is called, asked of the plane it moved in.
fn chat_called(app: &tauri::AppHandle, moved: &Moved) -> Option<String> {
    app.try_state::<Planes>()?
        .held(&moved.plane)
        .ok()?
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == moved.session)
        .map(|open| open.name)
}

/// Whether the operator is already looking at this chat.
///
/// **Three questions, and all three have to be yes.** The window is on screen and has the
/// keyboard; the window has THIS chat's plane in front; and that plane has this chat in front.
///
/// The middle one is the half #111 named as the opener's to close, and it was not pedantry:
/// every plane numbers its chats from one, so "is session 3 in front" has as many answers as
/// there are planes open, and a window showing plane B would have suppressed a notification
/// for plane A's chat 3 on the strength of plane A's own answer. The window is the only thing
/// that knows which plane it draws, so the window says (`window_shows_plane`), and
/// [`Showing`] is where it is kept.
///
/// Every unanswered question reads as "not looking", so a notification is sent rather than
/// suppressed. That is the cheap way round: one the operator did not need costs a glance, and
/// one they needed and did not get costs a chat sitting unanswered.
///
/// **The window asked is the one holding the chat's plane** (charter#126). With a project split
/// into a window of its own, "the window" is whichever one holds it, and asking the main window
/// would suppress a notification because the operator was looking at a different window.
fn already_looking_at(app: &tauri::AppHandle, moved: &Moved) -> bool {
    let Some(window) = app
        .try_state::<Showing>()
        .and_then(|showing| showing.holder(&moved.plane))
        .and_then(|label| app.get_webview_window(&label))
    else {
        return false;
    };
    if !window.is_visible().unwrap_or(false) || !window.is_focused().unwrap_or(false) {
        return false;
    }
    if !app
        .try_state::<Showing>()
        .is_some_and(|showing| showing.is_showing(window.label(), &moved.plane))
    {
        return false;
    }
    app.try_state::<Planes>()
        .and_then(|planes| planes.held(&moved.plane).ok())
        .is_some_and(|held| held.chats().front() == Some(moved.session))
}

/// The event a second launch sends the window: the directory it was run in, for the window to
/// open as a project.
///
/// The window and not the core, deliberately. Opening a plane is gated (ADR 0035) and the gate
/// ends in a dialog, so the one caller that can carry it through is the one with a window.
/// This handler resolving the plane and opening it here would be the second resolver and the
/// second gate — the `resolve`-against-`command_root` split that cost M2.16 a day, and the
/// `plane_root`-against-`find_root` split ADR 0034 was written to close.
pub(crate) const SECOND_LAUNCH: &str = "open-plane";

/// A second launch arrived: its directory goes to the window, which takes it through the same
/// opener every other path uses and opens it **as another project tab**.
///
/// Until tabs existed the window could only say so on screen — it already held a project, and
/// a second one had nowhere to go. That was the last of ADR 0033's "hands its plane to the
/// process already running" still unspent.
///
/// A directory charter cannot say anything about is not sent. The second process has already
/// exited by then, so there is nobody to tell and nothing on screen would explain a message
/// about a directory the operator is no longer standing in.
fn second_launch(app: &tauri::AppHandle, cwd: &str) {
    if cwd.is_empty() {
        return;
    }
    // To the main window alone: every window would otherwise open it as a tab of its own.
    // If another window already holds that project, the main window's open finds so and
    // raises that window instead (`show_window_holding`).
    let _ = app.emit_to(windows::MAIN, SECOND_LAUNCH, cwd);
}

/// A view a pane has open, and what its terminal has to match to show the session as it is:
/// the size the screen was drawn for, so what was wrapped stays wrapped, and how much history
/// the core is keeping, so the pane keeps the same.
#[derive(serde::Serialize, specta::Type)]
struct Watching {
    view: u32,
    columns: u16,
    rows: u16,
    scrollback: u32,
    /// What Shift+Enter sends: the newline of the harness the session runs
    /// (`Harness::newline`), or none for a shell, which keeps the terminal's own Enter.
    newline: Option<String>,
}

/// When this process started, as close to it as the app can see.
static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Says how far the launch has got, when `CHARTER_LAUNCH_LOG` is set.
///
/// A desktop app that hangs on the way up has nothing to show for it — no window, and on
/// some desktops not even an icon — and the interesting part happens before any of the app
/// can report anything. This is the thread to pull: each step, with the time it was
/// reached. Silent unless the variable is set, which nothing but a person debugging does.
fn reached(step: &str) {
    if charter_core::envvar::var_os("PURLIS_LAUNCH_LOG").is_some() {
        tracing::info!(
            "charter-launch {:>5} ms  {step}",
            STARTED.elapsed().as_millis()
        );
    }
}

/// How many CSS pixels of the leading edge macOS's window controls occupy.
///
/// The close, minimise and zoom buttons sit at x = 20, 40 and 60 and are 12 px across, so the
/// group ends at 72; this is that, rounded up to the next multiple of six so a title bar that
/// starts here is not one pixel off the last button. The window's own gutter is added on top
/// of it in `App.css` rather than folded in, because the gutter is the same on every platform
/// and this is not.
///
/// **A constant and not a measurement, because there is nothing to measure.** `NSWindow` gives
/// no public geometry for the button group, and the number has been 20/40/60 since Big Sur.
/// It is here rather than in the stylesheet so that the fact it is macOS's — not charter's —
/// is written where `cfg!(target_os)` decides it.
const MACOS_WINDOW_CONTROLS: u32 = 78;

/// What the operating system has already spent of the window's own title bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct TitleBarRoom {
    /// Whether charter's bar is drawn UNDERNEATH the system's window controls.
    ///
    /// True only where `tauri.conf.json`'s `titleBarStyle: "Overlay"` is honoured, which is
    /// macOS alone — every other platform ignores the key and keeps drawing its own title bar
    /// above the webview, so charter's bar is a row inside the window rather than the title
    /// bar itself, and nothing is reserved.
    pub overlaid: bool,
    /// How many CSS pixels at the leading edge the system's controls occupy, or zero.
    pub reserved: u32,
}

/// Where charter's title bar may start.
///
/// **Asked of the binary and never sniffed from a user agent.** The one fact that decides this
/// is whether `titleBarStyle: "Overlay"` in `tauri.conf.json` was honoured, and that is a
/// property of the target this binary was built for — which `cfg!` knows exactly and a
/// `navigator.userAgent` string only guesses at. It is also why this is a command rather than
/// a stylesheet constant: one frontend bundle is built per target by the same CI job that
/// builds the binary, but nothing in the bundle is told which target it landed in.
///
/// The window asks once, after the first frame, and the bar shifts right by
/// [`MACOS_WINDOW_CONTROLS`] when the answer lands. That settle is deliberate: the alternative
/// is awaiting an IPC round trip before `createRoot().render()`, which puts a command between
/// the process starting and the first frame — the one thing `main.tsx` is written to avoid
/// (ADR 0026's 2 s cold start). A title bar that finishes placing itself a millisecond after
/// it is drawn is the same settle the project strip, the workspace strip and the status line
/// all already have.
#[tauri::command]
#[specta::specta]
fn title_bar_room() -> TitleBarRoom {
    let overlaid = cfg!(target_os = "macos");
    TitleBarRoom {
        overlaid,
        reserved: if overlaid { MACOS_WINDOW_CONTROLS } else { 0 },
    }
}

/// Whether the first frame has been reported, so a webview reload is not a second launch.
static FIRST_FRAME: AtomicBool = AtomicBool::new(false);

/// The window says its first frame is on screen, which is where cold start ends.
///
/// It answers with the one line to put on screen when that took longer than the limit, with the
/// relaunch it suggests for Copy command (NO-4), and with nothing when it did not. An operator
/// who launched charter from an icon has no standard error to read, and a start that took half a
/// minute with no window has to say why somewhere they can see it (charter-app#24). Only the
/// first call is answered: a webview that reloads has not started the process again.
///
/// `CHARTER_BENCH_LOG` — which only `tools/bench.mjs` sets — also prints the number here.
#[tauri::command]
#[specta::specta]
fn first_frame() -> Option<slowstart::SlowStart> {
    let took = STARTED.elapsed();
    if charter_core::envvar::var_os("PURLIS_BENCH_LOG").is_some() {
        println!("charter-bench first-frame {}", took.as_millis());
    }
    if FIRST_FRAME.swap(true, Ordering::SeqCst) {
        return None;
    }
    slowstart::why(took, std::env::consts::OS)
}

/// What this launch had to go on, and the plane it opened — or the fact that it opened none.
///
/// **Never an error.** The working directory is a HINT: it is resolved once, at startup, to
/// decide which plane the first window opens, and after that a window's plane is explicit and
/// the working directory is never consulted again. A launch that resolved no plane leaves the
/// app running and holding nothing, which is a state the window draws rather than a failure
/// it reports.
#[tauri::command]
#[specta::specta]
fn plane_at_launch(launch: tauri::State<'_, Launch>) -> Launch {
    (*launch).clone()
}

/// Every plane this process is holding, by id.
///
/// There can be none, and none is an ordinary state: it is what an app launched outside any
/// plane comes up in, and what it returns to when the last project is closed.
#[tauri::command]
#[specta::specta]
fn open_planes(planes: tauri::State<'_, Planes>) -> Vec<PlaneId> {
    planes.open_now()
}

/// Lets go of a plane: its record is written, its sessions are ended, and its hook socket is
/// released.
///
/// **Nothing of the plane on disk goes.** Closing a project is the app letting go of it, and
/// a plane closed here can be opened again — by this process or another — with everything
/// still in it.
///
/// Its opposite is `opener::open_plane`, which is gated: opening a plane runs what its record
/// names, so it happens behind the operator's yes (ADR 0035). Closing one needs no gate — it
/// only ever does less.
#[tauri::command]
#[specta::specta]
fn close_plane(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<(), String> {
    planes.close(&plane)
}

/// One chat the app has open, as the UI draws it and as the quit warning lists it.
#[derive(serde::Serialize, specta::Type)]
struct OpenChat {
    session: u32,
    name: String,
    cwd: Option<String>,
    /// The harness it runs, by the word the plane calls it — or none for a shell.
    harness: Option<String>,
    /// Whether it is the chat to show: at a launch, the one that was in front at the quit.
    in_front: bool,
    /// The conversation it was resumed by, where it was. The UI says which happened.
    resumed: Option<String>,
    /// Why it is a new chat rather than the one it was, where it is.
    fresh: Option<String>,
    /// What a Resume from a session record had to guess because the record could not say it —
    /// its profile, its directory — or none (SI-8e).
    guessed: Option<String>,
    /// The harness profile it started on, where it started on one.
    profile: Option<String>,
    /// The persona it adopted.
    persona: Option<String>,
    /// What its harness cannot tell charter, said on the chat — none where it tells all.
    unreported: Option<String>,
    /// Its harness's card at a glance, which its header draws (HP-19) — none for a shell.
    card: Option<HarnessGlance>,
    /// Whether the operator pinned it (ADR 0039). It rides the plane's own app
    /// record, so a pinned chat comes back pinned at the next launch.
    pinned: bool,
    /// The name the operator gave it, or none — then its tab says the default, `<persona>
    /// <N>` (charter-app#254). Charter's label only: `name` is still what its harness was
    /// started with.
    label: Option<String>,
    /// Where a handoff opened it from, where one did: the note its tab's tooltip and its header
    /// draw, `↳ from steward 3 · ops` (charter-app#258). Never the parent's number.
    from: Option<HandedFromNote>,
}

/// Where a handed-off chat came from, as the window draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct HandedFromNote {
    /// The chat it came from, by the name the operator saw it under.
    pub name: String,
    /// The workspace it came from, or `plane root` for a chat that handed off from there
    /// (SI-1b) — `charter_core::active::Place::word`, drawn as it is.
    pub workspace: String,
}

impl From<&charter_core::reopen::HandedFrom> for HandedFromNote {
    fn from(from: &charter_core::reopen::HandedFrom) -> Self {
        Self {
            name: from.name.clone(),
            workspace: from.workspace.word().to_owned(),
        }
    }
}

/// One workspace as the sidebar draws it: what it is for, what it still means to do, and the
/// chats working in it.
#[derive(serde::Serialize, specta::Type)]
struct SidebarWorkspace {
    name: String,
    /// Where the workspace is, so a chat can be started in it.
    path: String,
    vision: String,
    todos: Vec<String>,
    chats: Vec<OpenChat>,
    /// Its colour as its `workspace.json` holds it — a palette name or `#rrggbb` — or `null`
    /// (charter-app#281). Here because every workspace tab draws its own, whether or not it is
    /// in front, and the sidebar is already the one read of every workspace.
    colour: Option<String>,
    /// Whether it is LIVE: its charter, memory and todos published with the plane
    /// (charter-app#301). Every place a workspace is drawn marks it.
    live: bool,
}

/// The whole left-hand side: every workspace with its chats, and the focused workspace's
/// persona and todos.
#[derive(serde::Serialize, specta::Type)]
struct Sidebar {
    root: String,
    workspaces: Vec<SidebarWorkspace>,
    /// The plane's personas, and the one a new chat here would adopt.
    personas: Vec<String>,
    persona: Option<String>,
    /// Chats whose directory is in no workspace, so the sidebar can still show them.
    unfiled: Vec<OpenChat>,
}

/// The sidebar, answered from the plane's model (FD-10b, [`charter_core::planemodel`]).
///
/// The model is read once when the plane is held, and from then on each change the watch
/// names re-reads only what it is part of: a todo closed in `beta` re-reads `beta/todos/`,
/// where this used to list every workspace's todos on every ask. A change nobody could name
/// reads the model again whole, so the plane on disk stays the truth.
///
/// The chats are the ones `Chats` already holds — one model of a chat, not a second derived
/// from the sessions. What files one under a workspace is the directory it works in, because
/// nothing on the plane records a chat: `.charter/frame/` belongs to the tmux frame and the
/// app stays out of it.
///
/// On a blocking thread and never the one that draws (SC-2): the model is behind a lock the
/// watch holds while it applies a change, and a change nobody could name reads every
/// workspace's todos again under it, so a plane with dozens of workspaces would hold the window
/// while it did.
#[tauri::command]
#[specta::specta]
async fn plane_sidebar(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Sidebar, String> {
    // Resolved here, so a plane that is not open refuses here; the blocking half carries the
    // plane's own handle, which outlives the registry's lock.
    let held = planes.held(&plane)?;
    off_the_window("reading the sidebar", move || sidebar_of(&held)).await
}

/// Runs `read` on a blocking thread, so a command that walks the plane's files never holds the
/// thread that draws the window (SC-2). `what` names it in the one failure this adds: the
/// thread ending without an answer.
async fn off_the_window<T: Send + 'static>(
    what: &str,
    read: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(read)
        .await
        .map_err(|err| format!("{what} did not finish: {err}"))?
}

/// The sidebar of the plane `held`, from its model ([`planes::Held::sidebar_model`]) and the
/// chats it holds.
fn sidebar_of(held: &planes::Held) -> Result<Sidebar, String> {
    let root = held.root();
    let on_disk = charter_core::workspaces::Plane::open(root);
    let model = held.sidebar_model();

    let mut filed: std::collections::HashMap<String, Vec<OpenChat>> =
        std::collections::HashMap::new();
    let mut unfiled = Vec::new();
    for chat in held.chats().open_now().into_iter().map(OpenChat::from) {
        match chat
            .cwd
            .as_deref()
            .and_then(|c| on_disk.workspace_of(std::path::Path::new(c)))
        {
            Some(name) => filed.entry(name).or_default().push(chat),
            None => unfiled.push(chat),
        }
    }

    let workspaces = model
        .rows()?
        .map(|row| SidebarWorkspace {
            path: row.path.display().to_string(),
            vision: row.vision.clone(),
            todos: row.todos.clone(),
            chats: filed.remove(&row.name).unwrap_or_default(),
            colour: row.colour.clone(),
            live: model.is_live(&row.name),
            name: row.name.clone(),
        })
        .collect();

    Ok(Sidebar {
        root: root.display().to_string(),
        workspaces,
        personas: model.personas()?,
        persona: model.default_persona().map(str::to_owned),
        unfiled,
    })
}

/// The focused workspace's panels: its repos, its todos and the plane's personas.
///
/// Its own command, separate from the repos below, because everything here is a directory
/// listing and a few small files. The panels paint the moment a workspace is focused, and
/// the part that has to run git arrives after — one command would make the todo list wait
/// for a status read on every clone.
///
/// **It names its plane**, like every other command here. It used to resolve one out of the
/// process's working directory — `plane::resolve`, the singleton ADR 0034 removed — so a
/// window showing a project the launch had not opened drew the workspaces of the one it had.
/// A workspace name means nothing without its project; two projects can both have an `alpha`.
///
/// **Served from the plane's model, per section** (FD-10c, [`charter_core::planemodel`]): a
/// workspace's clones, todos, memories and session records are read the first time it is
/// focused, and from then on each section again only when a change the watch names is part
/// of it. A memory an agent saves re-reads that workspace's `memory/` and nothing else.
///
/// On a blocking thread: a workspace's first ask reads every todo, memory and session record
/// of it.
#[tauri::command]
#[specta::specta]
async fn workspace_panels(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<panels::Panels, String> {
    let held = planes.held(&plane)?;
    off_the_window("reading the workspace's panels", move || {
        panels::served(&held, &workspace)
    })
    .await
}

/// The plane root's panels (SI-1, SI-8d): its session records, as the Sessions panel draws them.
/// A command of its own because the plane root is not a workspace, and `workspace_panels` asks
/// for one by name. On a blocking thread, as it reads every record.
#[tauri::command]
#[specta::specta]
async fn plane_root_panels(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<panels::PlaneRootPanels, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    off_the_window("reading the project's sessions", move || {
        Ok(panels::plane_root(&root))
    })
    .await
}

/// One session record, for its view tab (SI-8d): its facts and its text, read by its
/// plane-relative path through `sessionrecord::locate`, which refuses every other path. `null`
/// is a record that is not there any more.
#[tauri::command]
#[specta::specta]
fn session_record(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    path: String,
) -> Result<Option<panels::SessionRecordView>, String> {
    panels::session_record(planes.held(&plane)?.root(), &path)
}

/// Resumes a session from its record (SI-8d): a NEW chat in the record's place, on its harness,
/// given its conversation where it can be, and told the record in its briefing
/// (`charter_core::sessionresume`). The answer is the chat as the window draws it, whose
/// `resumed` or `fresh` says which happened.
///
/// `instead_of` is the window saying the chat it resumed this record into, by its number, ended
/// before its harness reported a session — the harness could not bring the conversation back —
/// so the same record starts fresh this time, and says so. It is **the same chat**, under its
/// id, in a run that begins `fresh` (ADR 0066), and not a second one. It may already be closed.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
fn resume_session(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    path: String,
    name: String,
    instead_of: Option<u32>,
    columns: u16,
    rows: u16,
) -> Result<OpenChat, String> {
    let held = planes.held(&plane)?;
    let resumed =
        charter_core::sessionresume::ready(held.root(), &path, &name, instead_of.is_some())?;
    let chat = Chat {
        program: resumed.ready.program.clone(),
        args: Vec::new(),
        cwd: resumed.ready.cwd.clone(),
        name,
        resume: resumed.ready.session.clone(),
        active: false,
        profile: resumed.start.profile.clone(),
        persona: resumed.start.persona.clone(),
        show_footer: false,
        pinned: false,
        number: None,
        label: None,
        from: None,
        renamed_from: None,
        ..Default::default()
    };
    let size = Size { columns, rows };
    let session = match instead_of {
        Some(instead_of) => {
            held.chats()
                .start_ready_instead_of(instead_of, &chat, &resumed.ready, size)?
        }
        None => held.chats().start_ready(&chat, &resumed.ready, size)?,
    };
    let open = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == session)
        .ok_or_else(|| format!("chat {session} ended as it started"))?;
    let mut drawn = OpenChat::from(open);
    // Which happened, in the words the window's note says after "came back as a new chat:".
    if let Some(why) = &resumed.fresh {
        drawn.resumed = None;
        drawn.fresh = Some(why.said());
    }
    // And what was guessed on the way, said beside it.
    drawn.guessed = (!resumed.notes.is_empty()).then(|| resumed.notes.join("; "));
    Ok(drawn)
}

/// What git says about each of the focused workspace's clones, and what the forge cache
/// last recorded for the branch each is on.
///
/// On a blocking thread and never the one that draws: a status read is bounded at five
/// seconds per clone, and a window that waited on it would miss the 100 ms a workspace
/// switch is allowed. **Nothing here crosses a network** — the forge state comes out of
/// `.charter/cache/glstate.json`, which charter-app reads and never writes.
#[tauri::command]
#[specta::specta]
async fn workspace_repos(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<panels::RepoStates, String> {
    // Resolved on the thread that asked, so the blocking half carries a path and not a
    // registry handle — and so a plane that is not open refuses here rather than inside a
    // thread whose failure would read as "reading the repos did not finish".
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || panels::repo_states(&root, &workspace))
        .await
        .map_err(|err| format!("reading the workspace's repos did not finish: {err}"))?
}

/// What is wrong in every project this process holds, project by project, for the alerts
/// drawer.
///
/// **Every project, never one**: an alert is about a plane rather than the workspace on
/// screen, and the drawer exists because alerts cross projects — so the command takes no plane,
/// and cannot be wired to the one in front by mistake.
///
/// On a blocking thread, because the plane-root alert asks git for a status per project and a
/// window that waited on eight of them would miss its frame.
#[tauri::command]
#[specta::specta]
async fn alerts_everywhere(
    planes: tauri::State<'_, Planes>,
) -> Result<Vec<alerts::PlaneAlerts>, String> {
    // Resolved here, so the blocking half carries paths and not a registry handle. A project
    // let go of between the listing and the lookup is simply not in the answer.
    let held: Vec<(PlaneId, PathBuf)> = planes
        .open_now()
        .into_iter()
        .filter_map(|plane| {
            let root = planes.held(&plane).ok()?.root().to_path_buf();
            Some((plane, root))
        })
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        held.into_iter()
            .map(|(plane, root)| alerts::of(plane, &root))
            .collect()
    })
    .await
    .map_err(|err| format!("reading the alerts did not finish: {err}"))
}

/// A harness's capability card at a glance (HP-19, W10, ADR 0072 §3): what the picker says under
/// the harness that is picked, what a chat's header draws, and what a control that is off for a
/// missing capability says. Every word is `charter_core::harness_card`'s, read off the harness's
/// declaration and the adapter charter ships for it; the whole card is the `harness` view.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
struct HarnessGlance {
    /// The word a profile's `kind` names: the key its card's view tab opens on.
    name: String,
    /// The product's own name, `Codex`: what a chat's header says.
    title: String,
    /// What the card is labelled: `What Codex can do here`.
    label: String,
    /// One line for each thing it lacks, in the card's order: none where it lacks nothing.
    lines: Vec<String>,
    /// What a control that types a prompt into its chat says while it is off: the card's line
    /// and label — or none where a prompt can be typed in.
    cannot_type: Option<String>,
}

impl From<&charter_core::harness_card::Card> for HarnessGlance {
    fn from(card: &charter_core::harness_card::Card) -> Self {
        Self {
            name: card.name.clone(),
            title: card.title.clone(),
            label: card.label(),
            lines: card.lines(),
            cannot_type: card.lacks(charter_core::harness_card::READY_TO_TYPE),
        }
    }
}

/// One row of the profile picker: what it runs, where charter read it, and what pressing
/// Enter on it would do.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
struct ProfileRow {
    name: String,
    kind: String,
    /// The environment and the command as one line a person reads, already contained: a
    /// profile is a file a chat can write, and a control byte in it must never redraw a row.
    shown: String,
    /// `built-in` or `charter.local.toml`.
    source: String,
    /// The row the picker starts on. It launches nothing by itself.
    is_default: bool,
    /// `new` or `changed` when this profile's command must be shown and approved before it
    /// runs; absent when charter has already recorded running exactly this.
    approval: Option<String>,
    /// Whether charter can type a prompt into a chat on it once its harness has started
    /// (`Harness::ready_to_type`): what a surface that types one, such as the first task
    /// (FR-28), offers it by.
    ready_to_type: bool,
    /// The card of the harness its `kind` names, at a glance (HP-19), or none for a kind this
    /// project has no declaration of.
    harness: Option<HarnessGlance>,
    /// What the sandbox does for a chat on it, where the project turned the sandbox on: the
    /// picker shows it beside "Start without the sandbox" (ADR 0067 §7, ruling V78 a). Null
    /// where the project has not, or the kind is no harness.
    sandbox: Option<sandboxing::SandboxAhead>,
}

/// Everything the picker draws, read from the plane when it is opened.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
struct StartOptions {
    profiles: Vec<ProfileRow>,
    /// Profiles charter read and will not use, by name and reason, so a row that is missing
    /// is never merely missing.
    refused: Vec<(String, String)>,
    personas: Vec<String>,
    /// The plane's `[persona] default`, which is the persona row the picker starts on.
    persona: Option<String>,
    /// Set when git would carry `charter.local.toml`: every declared profile is refused
    /// until it is fixed, and this is the one fix for that state.
    ignore_fix: Option<String>,
    /// Whether this plane declares no profiles of its own. The built-ins still start, and
    /// the picker says so rather than looking empty or broken.
    declares_none: bool,
}

/// What the picker draws: every profile this machine has, every one charter will not use,
/// and the plane's personas.
///
/// Read fresh every time it is opened, like the sidebar: `charter.local.toml` is a file the
/// operator edits by hand and a chat can write, so a cache here would be a second answer to
/// "what is on disk" that nothing invalidates.
///
/// **Off the main thread**: a sandboxed project's picker asks each approved Claude Code
/// profile's program its `--version` (ruling V87g), which can take seconds, and the window
/// must not freeze for it.
#[tauri::command]
#[specta::specta]
async fn start_options(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<StartOptions, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || start_options_in(&root))
        .await
        .map_err(|err| format!("charter could not read the profiles: {err}"))?
}

/// What [`start_options`] answers for the project at `root`.
fn start_options_in(root: &std::path::Path) -> Result<StartOptions, String> {
    let (set, check) = charter_core::profiles::for_launch(root);
    let on_disk = charter_core::workspaces::Plane::open(root);
    // Every harness the project has, read once for every row (HP-19).
    let cards = charter_core::harness_card::read(root);
    Ok(StartOptions {
        profiles: set
            .profiles()
            .iter()
            .map(|p| ProfileRow {
                name: p.name.clone(),
                kind: p.kind.clone(),
                shown: charter_core::profiletrust::shown(root, p),
                source: p.source.as_str().to_owned(),
                is_default: set.default.as_deref() == Some(p.name.as_str()),
                approval: charter_core::profiletrust::approval_needed(root, p)
                    .map(|a| a.as_str().to_owned()),
                ready_to_type: charter_core::harness::Harness::of_kind(&p.kind)
                    .and_then(charter_core::harness::Harness::ready_to_type)
                    .is_some(),
                harness: cards
                    .iter()
                    .find(|card| card.name == p.kind)
                    .map(HarnessGlance::from),
                sandbox: sandboxing::ahead_here(p, root),
            })
            .collect(),
        refused: set
            .refused
            .iter()
            .map(|r| {
                (
                    if r.name.is_empty() {
                        r.source.clone()
                    } else {
                        r.name.clone()
                    },
                    r.reason.clone(),
                )
            })
            .collect(),
        personas: on_disk.personas().map_err(|err| err.to_string())?,
        // Only a persona this plane HAS. `[persona] default` is a committed line that
        // nothing checks, so it can name a deleted persona or `_shared` — and preselecting
        // one the picker does not draw means the operator presses Start and is refused over
        // a persona they never chose.
        persona: charter_core::start::persona_for_a_new_chat(root),
        ignore_fix: (!check.passes()).then(|| check.fix.clone()),
        declares_none: set
            .profiles()
            .iter()
            .all(|p| p.source == charter_core::profiles::Source::BuiltIn),
    })
}

/// Records that the operator approved running this profile's command — **the one they were
/// shown**.
///
/// `shown` is the exact line the dialog drew. It is checked against the file again here,
/// and a mismatch refuses: between the picker reading the profile and the operator pressing
/// the button, `charter.local.toml` can change — it is gitignored, so an edit to it leaves
/// no diff for a reviewer to catch, and nothing stops a chat writing plane config. Without
/// this check the approval recorded whatever was on disk at CLICK time, so the operator
/// could approve, and charter could run, a command they never read. A review probe found
/// it, and it defeats the one prompt ADR 0022 exists to put in front of a launch.
///
/// Its own command, and a separate click from the one that starts the chat: this IS the
/// approval, and a command that both asked and ran would be asking nothing.
#[tauri::command]
#[specta::specta]
fn approve_profile(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
    shown: String,
) -> Result<(), String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    // Read through the LAUNCH read, so a profile in a file git would carry cannot be
    // approved into existence — the approval would be recorded and the launch would still
    // refuse, which is a yes that buys nothing.
    let (set, _check) = charter_core::profiles::for_launch(root);
    let profile = set.get(&name).ok_or_else(|| {
        format!(
            "no profile '{}' to approve",
            charter_core::shown::short(&name)
        )
    })?;
    charter_core::profiletrust::approve(root, profile, &shown)
}

/// Starts a chat on a harness profile, with a persona.
///
/// A command of its own rather than a flag on `open_session`, so neither can be mistaken
/// for the other by a caller passing null: this one goes through every gate a launch has,
/// and that one opens the operator's shell.
///
/// `boxes.show_footer` is the picker's footer checkbox, and it is a property of THIS chat
/// (ADR 0029). It reaches the harness as an environment variable set at the exec, so
/// it is decided here and nowhere later: Claude Code's footer command inherits the
/// environment its harness was started with, and no later click can change it.
///
/// `label` is the picker's optional Name field (charter-app#254): what the chat's tab says
/// instead of its default. It is held to the same rule a rename is, and **a refusal comes back
/// before anything starts**, so a name charter will not draw never costs a chat.
///
/// `boxes.new_branch` is the picker's "on a new branch" box (GL-1). When `cwd` is a repo's clone
/// and it is set, the chat starts on a branch of its own, cut for it and taken back if the start is
/// refused (`worktrees::on_a_branch`); anywhere else it changes nothing. The pane is told the
/// branch first, then whatever the start found to say.
///
/// **Off the main thread** (GL-1 review S3): cutting a branch checks a tree out, which takes
/// seconds on a large repo, and the window froze for it. Two starts in one clone are still
/// cut one at a time, by `chatpiece`'s lock per clone. Starting a session off the main thread
/// is what a relaunch's put-back already does.
// Over clippy's threshold, and it is a command's argument list: every one of these is a
// separate value the window sends. The picker's two boxes are folded into `Boxes`, because
// tauri-specta types at most ten arguments; the rest stay separate, as the window sends them.
// Not a doc comment, because the generated bindings carry those and this is about the Rust.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
async fn start_chat(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    profile: String,
    persona: Option<String>,
    cwd: Option<String>,
    name: String,
    label: Option<String>,
    boxes: Boxes,
    columns: u16,
    rows: u16,
) -> Result<Started, String> {
    let label = match label {
        Some(raw) => charter_core::reopen::label(&raw)?,
        None => None,
    };
    let held = planes.held(&plane)?;
    let config = planes.config().map(std::path::Path::to_path_buf);
    tauri::async_runtime::spawn_blocking(move || {
        let named = label.clone();
        let show_footer = boxes.show_footer;
        let without_sandbox = boxes
            .without_sandbox
            .map(|asked| charter_core::sandbox::OptOut {
                reason: asked.reason,
            });
        let (mut started, said) = worktrees::on_a_branch(
            held.root(),
            config.as_deref(),
            cwd.as_deref().map(std::path::Path::new),
            named.as_deref(),
            boxes.new_branch,
            |cwd| {
                start_chat_in(
                    &held,
                    profile,
                    persona,
                    cwd,
                    name,
                    label,
                    Picked {
                        show_footer,
                        without_sandbox,
                    },
                    columns,
                    rows,
                )
            },
        )?;
        started.notices.splice(0..0, said);
        Ok(started)
    })
    .await
    .map_err(|err| format!("charter could not start the chat: {err}"))?
}

/// The picker's two boxes, as the start reads them.
///
/// A struct because tauri-specta types a command of at most ten arguments, and `start_chat` grew
/// an eleventh with `new_branch`. The two boxes are the pair that belong together: both are the
/// operator's answer in the picker about this one chat, decided before it starts.
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
struct Boxes {
    /// Draw charter's footer in the pane (ADR 0029).
    show_footer: bool,
    /// Start on a branch of its own when the chat starts in a repo's clone (GL-1).
    new_branch: bool,
    /// "Start without the sandbox", for this chat only, and the reason typed, if any (ADR 0067
    /// §7, ruling V78 a). Null for a sandboxed start. The window's picker is the one place an
    /// opt-out is made: it reaches the core here, on the window's own human scope, and nothing
    /// a chat, a file or the CLI sends can carry one.
    without_sandbox: Option<WithoutSandbox>,
}

/// A person's opt-out, as the picker sends it.
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
struct WithoutSandbox {
    /// What the person typed as the reason, or null.
    reason: Option<String>,
}

/// What the picker's boxes decided about one chat, as its start reads them.
struct Picked {
    show_footer: bool,
    without_sandbox: Option<charter_core::sandbox::OptOut>,
}

/// The start itself, in the directory `on_a_branch` settled on.
#[allow(clippy::too_many_arguments)]
fn start_chat_in(
    held: &planes::Held,
    profile: String,
    persona: Option<String>,
    cwd: Option<PathBuf>,
    name: String,
    label: Option<String>,
    Picked {
        show_footer,
        without_sandbox,
    }: Picked,
    columns: u16,
    rows: u16,
) -> Result<Started, String> {
    let root = held.root();
    let start = charter_core::start::Start {
        profile: Some(profile.clone()),
        persona: persona.clone(),
        name: name.clone(),
        cwd,
        resume: None,
        show_footer,
        resuming: None,
        without_sandbox,
    };
    let ready = charter_core::start::ready(&start, root)?;
    let chat = Chat {
        program: ready.program.clone(),
        // What the RECORD keeps: the profile's own words, without charter's. A resume
        // spells them differently from a start, and both are decided again at the reopen.
        args: Vec::new(),
        cwd: ready.cwd.clone(),
        name,
        resume: ready.session.clone(),
        active: false,
        profile: Some(profile),
        persona,
        show_footer,
        // A chat is pinned by the operator afterwards, never at its start: a tab that
        // arrived already pinned would be an arrangement nobody made.
        pinned: false,
        // A chat the operator has just asked for has no number yet: `Sessions`
        // deals it one that this plane has never used (charter-app#90).
        number: None,
        label: label.clone(),
        from: None,
        renamed_from: None,
        ..Default::default()
    };
    let session = held
        .chats()
        .start_ready(&chat, &ready, Size { columns, rows })?;
    let mut notices = ready.notices;
    notices.extend(held.chats().start_notes(session));
    Ok(Started {
        session,
        label,
        notices,
        agents_md: ready
            .agents_md
            .into_iter()
            .map(TheirAgentsMd::from)
            .collect(),
    })
}

/// A chat that started: its session, and the name it was given as charter holds it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
struct Started {
    session: u32,
    /// The picker's Name field as the core's rule left it — trimmed, and none when it was
    /// blank — so the tab draws what the record holds rather than what was typed.
    label: Option<String>,
    /// What the start found to say, one line each, for the chat's pane (ADR 0085): why its
    /// `AGENTS.md` was not written, and an `AGENTS.md` charter's exclude line hides.
    notices: Vec<String>,
    /// The branches whose `AGENTS.md` a notice names as the operator's and hidden by charter's
    /// line: what the notice's Open file and Move aside… act on (NO-4).
    agents_md: Vec<TheirAgentsMd>,
}

/// A branch whose `AGENTS.md` is the operator's and hidden from `git status` by charter's line:
/// the repo's own folder (no piece) or one of its pieces.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
struct TheirAgentsMd {
    workspace: String,
    repo: String,
    piece: Option<String>,
}

impl From<charter_core::start::TheirAgentsMd> for TheirAgentsMd {
    fn from(at: charter_core::start::TheirAgentsMd) -> Self {
        Self {
            workspace: at.workspace,
            repo: at.repo,
            piece: at.piece,
        }
    }
}

/// Starts a session, and remembers it as a chat so a quit can write it down. No program is
/// the operator's shell.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
fn open_session(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    program: Option<String>,
    args: Vec<String>,
    cwd: Option<String>,
    name: String,
    columns: u16,
    rows: u16,
) -> Result<u32, String> {
    let program_named = program.is_some();
    let chat = unprofiled(program, args, cwd.map(PathBuf::from), name);
    // The board already knows about it: `Chats` announces a chat BEFORE its program starts,
    // so its very first hook lands somewhere. Registering it here would be too late.
    let chats = planes.held(&plane)?;
    let size = Size { columns, rows };
    // **The operator's own shell — no program named — is the one start the kill switch lets
    // through** (OV-1, ADR 0071). A program named here is anything at all, a harness included,
    // so it is refused while agents are stopped like every other chat.
    if program_named {
        chats.chats().start(&chat, size)
    } else {
        chats.chats().start_operator_shell(&chat, size)
    }
}

/// A shell tab in one folder of a branch (FM-10): the operator's own shell, as `open_session`
/// with no program starts it, in a folder the core resolved. `""` is the branch's own folder.
/// Refused, in the core's sentence, for a folder outside the branch, a link or git's own.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
fn open_shell_in_branch(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    folder: String,
    name: String,
    columns: u16,
    rows: u16,
) -> Result<u32, String> {
    let held = planes.held(&plane)?;
    let cwd = piecefiles::shell_folder(
        held.root(),
        piecefiles::branch(&workspace, &repo, &piece),
        &folder,
    )?;
    let chat = unprofiled(None, Vec::new(), Some(cwd), name);
    held.chats()
        .start_operator_shell(&chat, Size { columns, rows })
}

/// A chat on no profile: a program the window named, or the operator's shell when it named
/// none.
fn unprofiled(
    program: Option<String>,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    name: String,
) -> Chat {
    Chat {
        program: program.unwrap_or_else(sessions::shell),
        args,
        cwd,
        name,
        resume: None,
        active: false,
        // A chat opened through this command is not on a profile: it is the shell the app
        // opens, which is what this command is for. `start_chat` is the one that carries a
        // profile, and it is a command of its own so that neither can be mistaken for the
        // other by a caller passing null.
        profile: None,
        persona: None,
        // And it is not on a harness either, so there is no footer to keep or blank: this
        // path builds no charter environment at all (`Chats::start` passes an empty one).
        show_footer: false,
        pinned: false,
        // A chat the operator has just asked for has no number yet: `Sessions`
        // deals it one that this plane has never used (charter-app#90).
        number: None,
        label: None,
        from: None,
        renamed_from: None,
        ..Default::default()
    }
}

/// Ends a session and everything it started. It is no longer a chat a quit would record.
#[tauri::command]
#[specta::specta]
fn close_session(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<(), String> {
    planes.held(&plane)?.close_chat(session)
}

/// Drops a chat's request for the operator until it asks again — the needs-you item's Ignore
/// (charter-app#248). The chat is untouched: it is still waiting, and its next stop asks again.
#[tauri::command]
#[specta::specta]
fn ignore_needs_you(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<(), String> {
    planes.held(&plane)?.ignore_needs_you(session);
    Ok(())
}

/// The chats the app already has open — at a launch, the ones put back from the record.
///
/// The window asks this instead of opening its own: the core puts the record back, once the
/// window has sent the operator's answer to the launch's question (`opener::relaunch`,
/// charter-app#250), and a window that reloads asks again rather than starting a second copy.
#[tauri::command]
#[specta::specta]
fn opened_chats(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Vec<OpenChat>, String> {
    Ok(planes
        .held(&plane)?
        .chats()
        .open_now()
        .into_iter()
        .map(OpenChat::from)
        .collect())
}

/// The chats this launch could not start, by id, name and reason. They are still recorded, and
/// will be tried again at the next launch — or now, by Retry now — until the operator forgets
/// one.
#[tauri::command]
#[specta::specta]
fn chats_that_would_not_start(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<chats::NotStarted>, String> {
    Ok(planes.held(&plane)?.chats().would_not_start())
}

/// The chat `session` as the window draws it, once it has started: the answer of the commands
/// below that start one.
fn drawn(held: &planes::Held, session: u32) -> Result<OpenChat, String> {
    held.chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == session)
        .map(OpenChat::from)
        .ok_or_else(|| format!("chat {session} ended as it started"))
}

/// Retry now (NO-3): starts the chat with id `id` that this launch could not start, the way
/// the launch tried to. It is the chat as the window draws it, or why it still did not start —
/// and then it is still recorded, with that reason.
///
/// On a blocking thread, as `start_chat` is: a chat on a profile resolves its launch and checks
/// its program before it runs, and the window must not wait on that.
#[tauri::command]
#[specta::specta]
async fn retry_chat_that_did_not_start(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: String,
    columns: u16,
    rows: u16,
) -> Result<OpenChat, String> {
    let held = planes.held(&plane)?;
    tauri::async_runtime::spawn_blocking(move || {
        let session = held
            .chats()
            .retry(&id, held.root(), Size { columns, rows })?;
        drawn(&held, session)
    })
    .await
    .map_err(|err| format!("charter could not start the chat: {err}"))?
}

/// Forget this chat (NO-3): drops the chat with id `id` that this launch could not start from
/// the record. Kept otherwise, on purpose, so a moved directory never deletes a chat.
#[tauri::command]
#[specta::specta]
fn forget_chat_that_did_not_start(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: String,
) -> Result<(), String> {
    planes.held(&plane)?.chats().forget(&id)
}

/// Start fresh (NO-3): chat `session` started again on the plane's instructions as they are
/// now — the same chat, in a new run with no conversation resumed (ADR 0066). The answer is the
/// new one as the window draws it. The old one is ended here once the new one has started; a
/// refused start ends nothing. On a blocking thread, as `start_chat` is.
#[tauri::command]
#[specta::specta]
async fn start_chat_fresh(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    columns: u16,
    rows: u16,
) -> Result<OpenChat, String> {
    let held = planes.held(&plane)?;
    tauri::async_runtime::spawn_blocking(move || {
        let started = held.start_chat_fresh(session, Size { columns, rows })?;
        drawn(&held, started)
    })
    .await
    .map_err(|err| format!("charter could not start the chat: {err}"))?
}

/// Every chat this plane has open that is running on instructions the plane has changed since
/// it started (charter#369): its tab is marked, and the mark names the files. The window asks
/// again whenever the plane changes on disk.
#[tauri::command]
#[specta::specta]
fn chats_plane_updated(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<planes::PlaneUpdated>, String> {
    Ok(planes.held(&plane)?.plane_updated())
}

/// Says which chat is in front, so the record brings that one back in front.
#[tauri::command]
#[specta::specta]
fn chat_in_front(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: Option<u32>,
) -> Result<(), String> {
    planes.held(&plane)?.chats().bring_to_front(session);
    Ok(())
}

/// What the operator has pinned in one project (ADR 0039, stored per ADR 0040).
///
/// The project's own pin and its pinned workspaces come from the machine store; a pinned
/// CHAT is not here, because a chat pin rides that chat's own record and reaches the window
/// on `OpenChat::pinned` with the chat it is about.
#[derive(serde::Serialize, specta::Type)]
struct Pins {
    /// Whether this project itself is pinned.
    project: bool,
    /// Its pinned workspaces that still exist, in the order they were pinned in.
    ///
    /// The order the workspace strip draws them in (ADR 0054, charter#402): the operator's
    /// arrangement, where it used to be the plane's order filtered.
    workspaces: Vec<String>,
    /// Pins that no longer name a workspace on the plane — renamed, or removed.
    ///
    /// **Named rather than dropped silently**, and never drawn as a workspace: a window that
    /// drew one would be offering a workspace the plane does not have. This is ADR 0034's
    /// own hazard for a trust entry keyed on a path, one scope down.
    ///
    /// **A dormant pin** (V91c as amended, NO-1): kept in the store, in its place, and drawn
    /// again when its workspace comes back. The window says so with a Notice that offers
    /// Forget, so this names a pin only when the workspace is gone for certain (`pins_in`).
    missing: Vec<String>,
    /// Every pin, gone ones included, in the order the store keeps them.
    ///
    /// What an Undo of a Forget needs to put a pin back in its own place: `workspaces` and
    /// `missing` are each in order, but not in order with each other.
    order: Vec<String>,
    /// **Whether `missing` is the whole answer**: every pin the listing lacks was found gone.
    ///
    /// False when charter could not be sure of any of them — no root or `workspaces/`, a
    /// listing it could not read whole, a rename between its steps, a link whose target is
    /// away. `missing` then names only what is gone for certain, and may leave some out, so
    /// the window must not take a pin it lacks as one that came back: a dismissed Notice is
    /// let go only on a certain answer (NO-2, D-NO2-10).
    certain: bool,
}

/// What this operator has pinned in this project.
#[tauri::command]
#[specta::specta]
fn plane_pins(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Pins, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    Ok(pins_in(&planes.remembered().store, &root))
}

/// [`plane_pins`]'s answer for the plane at `root`, given the machine store.
///
/// **A pin is named gone only when the disk says so for certain** (NO-1), because the window
/// tells the operator about each one and offers to forget it (V91c as amended). Nothing is named gone when the root or its
/// `workspaces/` is not there (an unmounted volume, a checkout in progress), when the listing
/// could not be read whole, or while a workspace rename is between its steps (its journal).
/// And a name is gone only when NOTHING is at `workspaces/<name>`: a symlink whose target is
/// missing is unreadable, not gone, since an unmounted target comes back (D-NO1-9).
fn pins_in(store: &charter_core::machine::Store, root: &std::path::Path) -> Pins {
    // The plane's own list, read off the disk the same way the sidebar is: what workspaces
    // exist is the plane's answer and never the store's, so a pin is only ever matched
    // against it.
    let under = root.join("workspaces");
    let read = charter_core::workspaces::Plane::open(root).read_workspaces();
    let whole = under.is_dir() && matches!(&read, Ok((_, unread)) if unread.is_empty());
    let renaming = charter_core::wscmd::rename::in_flight(root);
    let there = read.map(|(names, _)| names).unwrap_or_default();
    let names: Vec<&str> = there.iter().map(String::as_str).collect();
    let (kept, missing) = store.pinned_workspaces(root, &names);
    let gone = |name: &String| {
        renaming
            .as_ref()
            .is_none_or(|(from, to)| name != from && name != to)
            && std::fs::symlink_metadata(under.join(name))
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    };
    let lacked = missing.len();
    let missing: Vec<String> = if whole {
        missing.into_iter().filter(gone).collect()
    } else {
        Vec::new()
    };
    Pins {
        project: store.recent(root).is_some_and(|entry| entry.pinned),
        workspaces: kept.into_iter().map(str::to_owned).collect(),
        // Certain only when every pin the listing lacks was found gone: one passed over (a
        // rename's, an unreadable link's) is a pin charter cannot say anything about.
        certain: whole && renaming.is_none() && missing.len() == lacked,
        missing,
        order: store
            .recent(root)
            .map(|entry| entry.pinned_workspaces.clone())
            .unwrap_or_default(),
    }
}

/// Pins or unpins the project itself.
#[tauri::command]
#[specta::specta]
fn pin_project(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    pinned: bool,
) -> Result<(), String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    planes.pin(&root, None, pinned)
}

/// Pins or unpins one workspace inside a project.
#[tauri::command]
#[specta::specta]
fn pin_workspace(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    pinned: bool,
) -> Result<(), String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    planes.pin(&root, Some(&workspace), pinned)
}

/// Puts a project's pinned workspaces in the order the operator dragged them into on the
/// workspace strip (SI-6). Only the order moves: a name that is not pinned is passed over, and
/// pinning stays [`pin_workspace`]'s.
#[tauri::command]
#[specta::specta]
fn arrange_workspace_pins(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspaces: Vec<String>,
) -> Result<(), String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    planes.arrange_workspaces(&root, &workspaces)
}

/// Pins or unpins one chat.
///
/// Its own command rather than a third case of the two above, because it is written
/// somewhere else entirely: a chat pin goes in the plane's own `.charter/app/reopen.json`
/// and never in the machine store, which ADR 0034 forbids holding a chat's name.
#[tauri::command]
#[specta::specta]
fn pin_chat(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    pinned: bool,
) -> Result<(), String> {
    planes.held(&plane)?.chats().pin(session, pinned)
}

/// The order the chat strip draws this project's chats in, by session, so the record lists
/// them in it and the next launch — or a reloaded window — puts them back in it (SI-6).
///
/// **In the plane's own `.charter/app/reopen.json`, beside each chat's pin**, and never in the
/// machine store, for [`pin_chat`]'s reason: a chat is numbered per plane, and ADR 0034 keeps
/// its number out of a file every plane shares. That file is out of git, so the order is this
/// machine's as a pin is.
#[tauri::command]
#[specta::specta]
fn chat_order(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    sessions: Vec<u32>,
) -> Result<(), String> {
    planes.held(&plane)?.chats().hold_order(sessions);
    Ok(())
}

/// Gives one chat a name, or takes the one it was given off with a blank — and answers the name
/// it now has, so the tab draws what charter holds rather than what was typed (charter-app#254).
///
/// **Charter's label, never the harness's**: the program keeps the `--name` it was started
/// with, so renaming a chat never disturbs one that is running. The name goes in the plane's
/// own `.charter/app/reopen.json`, beside the chat's pin, so it comes back at a relaunch.
#[tauri::command]
#[specta::specta]
fn rename_chat(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    label: String,
) -> Result<Option<String>, String> {
    planes.held(&plane)?.chats().rename(session, &label)
}

/// Asks the app to quit, the way the menu's Quit and the tray's do.
///
/// It is the same function they call, so what this goes through is the real path: the ask
/// is counted, the window is shown, and the window is sent `quit-asked`. The command
/// palette's own quit action is this too.
#[tauri::command]
#[specta::specta]
fn ask_to_quit(app: tauri::AppHandle) {
    lifecycle::ask_to_quit(&app);
}

/// The window's answer to being asked to quit: go.
#[tauri::command]
#[specta::specta]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

/// The window's answer to being asked to quit: not now. The next ask warns again.
#[tauri::command]
#[specta::specta]
fn quit_cancelled(quitting: tauri::State<'_, Quitting>) {
    quitting.never_mind();
}

/// Hides the window, which is what its close button does. Every session keeps running.
#[tauri::command]
#[specta::specta]
fn hide_window(window: tauri::Window) {
    let _ = window.hide();
}

/// Whether the window is on screen. The scenario tests ask; nothing in the UI does.
#[tauri::command]
#[specta::specta]
fn window_showing(window: tauri::Window) -> bool {
    window.is_visible().unwrap_or(false)
}

impl From<chats::Open> for OpenChat {
    fn from(open: chats::Open) -> Self {
        Self {
            session: open.session,
            name: open.name,
            cwd: open.cwd.map(|cwd| cwd.display().to_string()),
            harness: open.harness.map(Harness::name).map(str::to_owned),
            unreported: open
                .harness
                .and_then(Harness::unreported)
                .map(str::to_owned),
            card: open.harness.map(|harness| {
                HarnessGlance::from(&charter_core::harness_card::built_in_card(harness))
            }),
            profile: open.profile,
            persona: open.persona,
            in_front: open.in_front,
            pinned: open.pinned,
            label: open.label,
            from: open.from.as_ref().map(HandedFromNote::from),
            guessed: None,
            resumed: match &open.how {
                Reopened::Resumed(id) => Some(id.to_string()),
                Reopened::Fresh(_) => None,
            },
            fresh: match &open.how {
                Reopened::Resumed(_) => None,
                // The words the pane shows. They say what charter knows and no more: not
                // that there was no conversation, but that nothing recorded one.
                Reopened::Fresh(Fresh::NoConversationRecorded) => {
                    Some("no conversation was recorded for it".to_owned())
                }
                Reopened::Fresh(Fresh::NoResumeForThisProgram) => {
                    Some("charter has not measured how this program resumes".to_owned())
                }
                Reopened::Fresh(Fresh::SessionNamedByTheOperator) => {
                    Some("its own arguments name a session, so charter added none".to_owned())
                }
                Reopened::Fresh(Fresh::WorkspaceRenamed) => Some(
                    "its workspace was renamed, and Claude Code keeps a conversation under the \
                     folder it ran in"
                        .to_owned(),
                ),
            },
        }
    }
}

/// What every chat is doing, and which of them are asking for you.
///
/// The window asks once, when it opens; after that it is told (`chat-moved`). A chat the app
/// has never heard from is `unknown`, which is what the spec says a harness with no state
/// hook shows.
#[tauri::command]
#[specta::specta]
fn chat_states(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Vec<Moved>, String> {
    let held = planes.held(&plane)?;
    Ok(held
        .chats()
        .open_now()
        .into_iter()
        .map(|open| held.board().now(open.session))
        .collect())
}

/// Sends what a pane typed to the session's program. Anything but the terminal's own answer
/// drops a curation prompt still waiting to be typed into it (`Held::operator_input`).
#[tauri::command]
#[specta::specta]
fn send_input(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    text: String,
) -> Result<(), String> {
    planes
        .held(&plane)?
        .operator_input(session, text.as_bytes())
}

/// Sends a pane's bytes that are not text to the session's program, each as the one byte it
/// is: a mouse report in the default encoding, which xterm hands over one character per byte
/// (charter#493). Sent as text, a byte above 127 would reach the program as two.
#[tauri::command]
#[specta::specta]
fn send_input_bytes(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    bytes: Vec<u8>,
) -> Result<(), String> {
    planes.held(&plane)?.operator_input(session, &bytes)
}

/// Tells a session how big the pane showing it now is.
#[tauri::command]
#[specta::specta]
fn resize_session(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    columns: u16,
    rows: u16,
) -> Result<(), String> {
    planes
        .held(&plane)?
        .chats()
        .sessions()
        .resize(session, Size { columns, rows })
}

/// Opens a view of a session for a pane that is now on screen: the channel is sent the screen
/// as it already is, and then the session's output. Answers with the id that closes the view.
#[tauri::command]
#[specta::specta]
fn watch_session(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    output: Channel<String>,
) -> Result<Watching, String> {
    let held = planes.held(&plane)?;
    let chats = held.chats();
    let newline = chats.harness(session).map(|h| h.newline().to_owned());
    let watching = chats.sessions().watch(
        session,
        // A view whose window has gone is closed by the pane that owned it; until then, text
        // it cannot take is dropped rather than held, and the session keeps running.
        Box::new(move |text| {
            let _ = output.send(text);
        }),
    )?;
    Ok(Watching {
        view: watching.view,
        columns: watching.size.columns,
        rows: watching.size.rows,
        scrollback: sessions::SCROLLBACK,
        newline,
    })
}

/// Closes a view, for a pane that has gone off screen. The session keeps running.
#[tauri::command]
#[specta::specta]
fn unwatch_session(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    view: u32,
) -> Result<(), String> {
    planes
        .held(&plane)?
        .chats()
        .sessions()
        .unwatch(session, view)
}

/// The sessions that are running, in the order they were opened.
#[tauri::command]
#[specta::specta]
fn running_sessions(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Vec<u32>, String> {
    Ok(planes.held(&plane)?.chats().sessions().running())
}

/// Hands `ipc_commands.rs`'s list, both classes of it, to `tauri-specta`.
macro_rules! register {
    (
        value_free: [$($($free:ident)::+),* $(,)?],
        vault_values: [$($($value:ident)::+),* $(,)?] $(,)?
    ) => {
        collect_commands![$($($free)::+,)* $($($value)::+),*]
    };
}

/// Every command the UI can call, from the one list in `ipc_commands.rs`: the source of the
/// handler, of the TypeScript the UI imports, and — through `build.rs` — of the allow-list that
/// decides which window may call which (ADR 0052).
fn commands() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(app_commands!(register))
        // What `update://checked` carries. It crosses on an event rather than a command, so it
        // is named here or the window would have to write the shape out by hand.
        .typ::<updates::Offer>()
        // What `plane-changed` carries, for the same reason.
        .typ::<planewatch::PlaneChanged>()
        // What `files-changed` carries (FM-1).
        .typ::<filewatch::FilesChanged>()
        // What `branch-changed` carries (FM-4).
        .typ::<branchwatch::BranchChanged>()
        // What `files-searched` carries (FM-8).
        .typ::<searchfiles::FilesSearched>()
        // What `extension-heard` carries (charter-app#343).
        .typ::<heard::ExtensionHeard>()
        // What `harness-by-hand` carries (ADR 0062).
        .typ::<hooks::ByHand>()
        // What `chat-touching` carries (FM-6).
        .typ::<hooks::ChatTouching>()
        // What `smart-close` carries (ADR 0064).
        .typ::<smartclose::SmartClosing>()
        // The event a launch without the session bus is told the bus answers on (`portal.rs`),
        // named once for both sides.
        .constant("SESSION_BUS_ANSWERS", portal::ANSWERS)
}

/// Where the generated TypeScript lives.
///
/// Anchored to this crate's own directory, not to the working directory. A debug build
/// writes it at every start, and the app is started from wherever the operator is — so a
/// relative path scatters a `src/bindings.ts` beside every plane, every temp directory a
/// test runs in, and anywhere else the app is launched from. One such file was committed
/// before this was noticed.
const BINDINGS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");

/// How the TypeScript is written, so that the app and the test that guards it agree.
fn typescript() -> specta_typescript::Typescript {
    specta_typescript::Typescript::default()
}

/// Where the UI RPC's typed client lives (FD-26): the same commands, over the link to `charterd`.
#[cfg(test)]
const UI_RPC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/uiRpc.ts");

/// The import every generated command calls through: Tauri's IPC.
#[cfg(test)]
const TAURI_INVOKE: &str =
    "import { invoke as __TAURI_INVOKE, Channel } from \"@tauri-apps/api/core\";";

/// The UI RPC's typed TypeScript client (ADR 0068 §4, FD-26): what `tauri-specta` generates
/// from `ipc_commands.rs`, with each command sent by `uiRpcLink.ts` on the link's control lane
/// instead of over Tauri's IPC. One generator, so the two clients cannot disagree about a
/// command, its arguments or its types; only the transport differs.
#[cfg(test)]
fn ui_rpc_client() -> String {
    let out = tempfile::tempdir().expect("a directory to generate into");
    let generated = out.path().join("bindings.ts");
    commands()
        .export(typescript(), &generated)
        .expect("the bindings are generated");
    let bindings =
        std::fs::read_to_string(&generated).expect("the generated bindings are readable");
    assert!(
        bindings.contains(TAURI_INVOKE),
        "tauri-specta no longer imports Tauri's invoke as the UI RPC's client expects"
    );
    let client = without_channel_commands(&bindings);
    // `Channel` stays imported only while something the client keeps still names it.
    let names = if client.contains("Channel<") {
        "invoke as __TAURI_INVOKE, Channel"
    } else {
        "invoke as __TAURI_INVOKE"
    };
    client.replacen(
        TAURI_INVOKE,
        &format!(
            "// The UI RPC's client (FD-26): bindings.ts's commands, sent over the link to \
             charterd.\nimport {{ {names} }} from \"./uiRpcLink\";"
        ),
        1,
    )
}

/// The generated `commands` object without each command that takes a `Channel`: a channel is
/// Tauri's stream to the window, and on the link a terminal's bytes are a view of the session
/// protocol instead (ADR 0068 §4).
///
/// A command starts at a line `\t<name>: (<args>) => …` directly inside the object, with the
/// doc comment right above it, and runs to the next one's doc comment. A return type's own
/// fields sit at the same indent but are never `(…) => …`, so only a command starts one.
#[cfg(test)]
fn without_channel_commands(bindings: &str) -> String {
    let lines: Vec<&str> = bindings.split_inclusive('\n').collect();
    let Some(open) = lines
        .iter()
        .position(|l| l.starts_with("export const commands = {"))
    else {
        return bindings.to_owned();
    };
    // `};` alone on a line: a return type's own `}` at the margin is always followed by more.
    let close = (open + 1..lines.len())
        .find(|&i| lines[i].trim_end() == "};")
        .unwrap_or(lines.len());
    let starts_a_command = |line: &str| {
        line.strip_prefix('\t').is_some_and(|rest| {
            let name_end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(0);
            name_end > 0 && rest[name_end..].starts_with(": (") && rest.contains(") => ")
        })
    };
    let is_doc = |line: &str| {
        let line = line.trim_start();
        line.starts_with("/**") || line.starts_with('*')
    };
    // Where each command's text begins: its doc comment, or the command itself.
    let mut begins: Vec<usize> = (open + 1..close)
        .filter(|&i| starts_a_command(lines[i]))
        .map(|mut i| {
            while i > open + 1 && is_doc(lines[i - 1]) {
                i -= 1;
            }
            i
        })
        .collect();
    begins.push(close);
    let mut out: String = lines[..=open].concat();
    out.push_str(&lines[open + 1..begins[0]].concat());
    for pair in begins.windows(2) {
        let command = lines[pair[0]..pair[1]].concat();
        let window_only = charter_session_protocol::ui::WINDOW_ONLY
            .iter()
            .any(|name| command.contains(&format!("(\"{name}\"")));
        if !command.contains("Channel<") && !window_only {
            out.push_str(&command);
        }
    }
    out.push_str(&lines[close..].concat());
    out
}

/// The shared lock this app holds on the config home for its life (`renamelocal::busy::LOCK`).
struct HoldsTheConfigHome(#[allow(dead_code)] std::fs::File);

/// What the launch's rename-local said, for the app's log.
#[cfg(not(feature = "e2e"))]
fn log_the_rename(renamed: &charter_core::renamelocal::Moved) {
    match &renamed.refused {
        Some(why) => tracing::warn!("charter: the local rename waits: {why}"),
        None => {
            for line in &renamed.said {
                tracing::info!("charter: rename-local: {line}");
            }
            if !renamed.complete {
                tracing::warn!(
                    "charter: rename-local left some old names in place; they are still read"
                );
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before anything else, chats included: the host has no controlling terminal, because a
    // level-3 agent shares its session and could open it (V77, ADR 0080 §1). Started straight
    // from a shell, the app runs again as its own child, which leaves; this process only
    // waits for it and exits as it does.
    let left = charter_core::noterminal::leave();
    if let Ok(charter_core::noterminal::Left::Relaunched(code)) = left {
        std::process::exit(code);
    }
    // Read first, so that what it holds is when the process started and not when the window
    // first asked.
    LazyLock::force(&STARTED);
    // Before anything opens a descriptor: an app launchd started (the Finder, the Dock) has a
    // soft limit of 256, and 200 chats hold more than that (ADR 0068 §9, SC-15).
    let raised = charter_core::openfiles::raise();
    // Before anything that can panic: a panic that ends the app is written down on its way
    // out, where one that went to a standard error nobody reads was lost (charter-app#16).
    panics::record();
    // Next, so everything the app notices from here on is kept in a file as well as said on
    // a standard error that, launched from the Dock, nobody reads (#647).
    charter_core::applog::install();
    // A variable that chooses the project, named twice with two values, is refused rather
    // than read under one of its names (D-RN2d-8). The sentence names the remedy, both names
    // set to the same value. It goes to the app's log, which a terminal launch also sees on
    // standard error, and the app does not start: every window it opened would act on a
    // project it guessed.
    if let Some(disagree) = charter_core::envvar::disagreement() {
        tracing::error!("charter: {disagree}");
        std::process::exit(2);
    }
    reached("run() entered");
    reached(&raised.to_string());
    match &left {
        Ok(left) => reached(&format!("the controlling terminal: {left:?}")),
        // Not fatal here: a level-3 chat checks again before it starts, and refuses to start
        // while the host still has a terminal (`charter_core::acp::NotStarted::Terminal`).
        Err(err) => tracing::warn!("charter: the app could not leave its terminal: {err}"),
    }
    if matches!(raised, charter_core::openfiles::Raised::Refused { .. }) {
        tracing::warn!("charter: {raised}");
    }
    // Before anything touches GTK: a desktop portal the session bus is still trying to start
    // costs GTK 25 s and WebKitGTK 5 more (charter-app#24). Asked here for 300 ms; when it is
    // silent this does not return — the launch starts again in place without the bus.
    #[cfg(target_os = "linux")]
    portal::start_clear_of_a_silent_portal();
    reached("the desktop portal is asked");
    let commands = commands();

    #[cfg(debug_assertions)]
    commands
        .export(typescript(), BINDINGS)
        .expect("the TypeScript bindings are written");
    reached("the bindings are written");

    reached("building");
    // Everything slow about a launch happens inside `build()`: the window is created there,
    // and on a Linux session whose desktop portal cannot start, GTK waits out 25 s of D-Bus
    // and WebKitGTK another 5 before any of it (charter-app#24). Nothing charter can say is
    // on screen yet — there is no screen — so it is said in the log and on standard error, from a thread,
    // while the wait is still going on. `built` below lets the thread go.
    let (built, still_building) = std::sync::mpsc::channel::<()>();
    // A machine with no thread to spare still starts; it just starts without the warning.
    let _ = std::thread::Builder::new()
        .name("charter-slow-start".into())
        .spawn(move || {
            slowstart::while_it_waits(
                &still_building,
                slowstart::LIMIT,
                std::env::consts::OS,
                &mut |line| tracing::warn!("{line}"),
            );
        });

    let app = tauri::Builder::default()
        // First, so a second launch is handed to the app already running rather than
        // starting a second one — which would be a second set of sessions on the same plane.
        //
        // **And its directory is handed over with it.** The arguments and the working
        // directory used to be bound to `_` and dropped, so `charter` typed inside a second
        // plane raised a window showing the first — the operator's ask thrown away at the
        // door. ADR 0033: a second launch hands its plane to the process already running,
        // which raises the window holding that plane or opens one for it.
        .plugin(tauri_plugin_single_instance::init(|app, _args, cwd| {
            lifecycle::show(app);
            second_launch(app, &cwd);
        }))
        .plugin(tauri_plugin_opener::init())
        // No webview navigates off the app, whatever a page it draws links to (FM-2 review).
        .plugin(navguard::plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        // Registered in every build so a broken updater config fails CI's app build, and
        // inert until asked: nothing checks unless `updates::watch` or a command does.
        .plugin(tauri_plugin_updater::Builder::new().build());

    // What the scenario tests drive the window through. The feature is off in every build
    // anyone is given, so nothing here can be reached in one.
    #[cfg(feature = "e2e")]
    let app = app
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    app.invoke_handler(commands.invoke_handler())
        .menu(lifecycle::menu)
        .on_menu_event(|app, event| lifecycle::clicked(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            // The close button hides the window. Every session is a child of this process
            // (ADR 0025), so a close that ended them would end the day's work; the way out
            // is Quit, which says what it is about to end.
            //
            // A split window closes instead, and hands its projects back to the main window
            // with every chat still running (ADR 0033, amended 2026-09-26).
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    windows::close_requested(window, api);
                }
                tauri::WindowEvent::Destroyed => {
                    // Its expanded folders are no longer watched for it (FM-1).
                    if let Some(watch) = window.try_state::<filewatch::FileWatch>() {
                        watch.forget(window.label());
                    }
                    // And its ⌘P listings are let go of (FM-7).
                    if let Some(finder) = window.try_state::<findfiles::FileFinder>() {
                        finder.forget(window.label());
                    }
                    // Nor the branches whose changes it shows (FM-4).
                    if let Some(watch) = window.try_state::<branchwatch::BranchWatch>() {
                        watch.forget(window.label());
                    }
                    // And its ⌘⇧F searches stop (FM-8).
                    if let Some(searches) =
                        window.try_state::<std::sync::Arc<searchfiles::FileSearches>>()
                    {
                        searches.forget(window.label());
                    }
                    windows::destroyed(window);
                }
                _ => {}
            }
        })
        .setup(|app| {
            reached("setup");
            // One charter per user even without a session bus (`instance.rs`). After the
            // single-instance plugin, which has already handed an ordinary second launch over;
            // before the window and before any plane, whose hook socket `hookwire` removes
            // when it is stale on the promise that no second app is running.
            #[cfg(target_os = "linux")]
            instance::one_per_user(app);
            // This machine's local state moves to the purlis names (RN-5, V93f): after the
            // single-instance handoff and the one-per-user lock, so a second launch never
            // migrates, and before any project, chat or watch opens a file under one. Anything
            // else of charter's still running makes it wait for the next launch (D-RN5-11).
            // Off in the scenario build, whose specs name the old folders (D-RN5-7).
            #[cfg(not(feature = "e2e"))]
            if let Some(renamed) = charter_core::renamelocal::at_launch(&app.config().identifier) {
                log_the_rename(&renamed);
            }
            // From here on this app holds the config home: no rename-local of a later launch,
            // or of a terminal, moves it while the app runs.
            if let Some(root) = charter_core::machine::config_root()
                && let Some(held) = charter_core::renamelocal::busy::hold_shared(&root)
            {
                app.manage(HoldsTheConfigHome(held));
            }
            // How this launch stands with the session bus, for the window's notice
            // (`portal.rs`): nothing to say on one that has it.
            app.manage(portal::SessionBus::of(
                std::env::var(portal::SESSION_BUS).ok().as_deref(),
                charter_core::envvar::var(portal::SESSION_BUS_KEPT).as_deref(),
                std::env::var_os("XDG_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .as_deref(),
            ));
            #[cfg(target_os = "linux")]
            portal::listen_again(app.handle());
            // **The window, built here rather than by Tauri from the config, so it can be
            // handed the operator's layout and theme as it is created** (`windowprefs.rs`). Its
            // entry in `tauri.conf.json` says `"create": false` and is still the one source of
            // its size, title and title bar; this adds the initialization script and nothing
            // else. First in `setup`, which is exactly where Tauri would have built it.
            let main = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == lifecycle::WINDOW)
                .cloned()
                .ok_or("tauri.conf.json declares no main window")?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), &main)?
                .initialization_script(windowprefs::creation_script(
                    charter_core::machine::config_root().as_deref(),
                ))
                .on_new_window(navguard::no_new_window)
                .build()?;
            reached("the window is built");
            // Where a panic is kept, now that the app can be told where its logs belong. An app
            // with no log directory still has standard error, which is all it had before.
            // Tauri's folder for this identifier, or the purlis one rename-local moved it to.
            if let Some(logs) = charter_core::applog::log_dir_for(&app.config().identifier) {
                panics::keep_in(&logs);
            }
            app.manage(Quitting::default());
            app.manage(updates::Installed::default());
            // The extension executor (ADR 0041 stage 2). Managed for the table of
            // programs it is running, which `Exit` below empties. It starts the app's own
            // built-in extensions, found here in its resources and nowhere else
            // (charter-app#339).
            let built_in = extensions::find_built_in(app.path().resource_dir().ok());
            extensions::keep_built_in(built_in.clone());
            // The executor events are delivered with, and the notes they left
            // (charter-app#343): the built-ins hear what they declare, as any extension does.
            app.manage(heard::Heard::with_built_in(built_in.clone()));
            app.manage(views::Views::with_built_in(built_in));
            app.manage(changes::Busy::default());
            // The folders each window's explorer has expanded, watched so an agent's new file
            // reaches the tree (FM-1).
            app.manage(filewatch::FileWatch::new({
                let app = app.handle().clone();
                std::sync::Arc::new(move |window: &str, folders| {
                    let _ = app.emit_to(
                        window,
                        filewatch::CHANGED,
                        filewatch::FilesChanged { folders },
                    );
                })
            }));
            // Each window's ⌘P session: the branches it has listed while the palette is up (FM-7).
            app.manage(findfiles::FileFinder::default());
            // The branches each window shows the changes of, listened to whole so an agent's
            // first write anywhere in one moves its markers (FM-4).
            app.manage(branchwatch::BranchWatch::new(
                {
                    let app = app.handle().clone();
                    std::sync::Arc::new(move |window: &str, branches| {
                        let _ = app.emit_to(
                            window,
                            branchwatch::CHANGED,
                            branchwatch::BranchChanged { branches },
                        );
                    })
                },
                reader(),
            ));
            // The clones whose save standing is shared ask the same watch to listen to their
            // working trees (FD-11), so an idle clone's standing costs no `git status`.
            {
                let app = app.handle().clone();
                charter_core::standings::watch_with(std::sync::Arc::new(move |wanted| {
                    if let Some(watch) = app.try_state::<branchwatch::BranchWatch>() {
                        watch.want(wanted);
                    }
                }));
            }
            // Each window's ⌘⇧F searches, one per Search tab, and how their hits reach the
            // window that asked (FM-8).
            app.manage(std::sync::Arc::new(searchfiles::FileSearches::default()));
            app.manage(searchfiles::SearchTeller({
                let app = app.handle().clone();
                std::sync::Arc::new(move |window: &str, batch| {
                    let _ = app.emit_to(window, searchfiles::HEARD, batch);
                })
            }));
            // What each window is holding, and which of its projects it has in front. Empty
            // until a window says, and an empty answer means "not looking", so a notification
            // is sent rather than suppressed.
            app.manage(Showing::default());
            // The clipboard a vault's Copy writes to, and what it wrote, for the clear a minute
            // later and the one at exit.
            app.manage(vaults::SystemClipboard::default());

            // Which `charter` a hook runs. Without one, nothing is armed and every chat
            // reads `unknown` — never a hook pointed at a path that is not there. It is a
            // property of this build and not of a project, so every plane arms with it.
            let binary = charter_binary();
            if binary.is_none() {
                tracing::warn!(
                    "charter: no `charter` binary beside the app, so no chat can report its \
                     state; every one will show as unknown"
                );
            }
            let plugin = bundled_plugin(app.handle());
            if plugin.is_none() {
                tracing::warn!(
                    "charter: no plugin in the app's resources, so a Claude Code chat is started \
                     without charter's hooks, guard or skills; every one will show as unknown"
                );
            }
            // What a shell tab finds first on its `PATH` (ADR 0062), written at every launch so
            // each shim runs THIS build's `charter`. None without a `charter` for them to run.
            let shims = binary
                .as_deref()
                .and_then(|binary| shell_tab_shims(app.handle(), binary));
            // The git hooks every harness chat commits through (SQ-16), written at every launch
            // for the same reason as the shims.
            let git_hooks = binary
                .as_deref()
                .and_then(|binary| chat_git_hooks(app.handle(), binary));
            // The copy `charter plugin install` made for chats started outside the app, brought
            // up to date with this build (#449). Off the main thread: it reads and writes a few
            // files, and nothing on screen waits for it.
            if let (Some(binary), Some(plugin)) = (binary.clone(), plugin.clone()) {
                refresh_installed_plugin(binary, plugin);
            }
            // The registry is managed BEFORE a plane is opened, because opening one starts
            // programs, and a program that dies at once tells the board, which tells the
            // window, which asks this registry what the chat is called.
            app.manage(
                Planes::telling(
                    {
                        let window = app.handle().clone();
                        std::sync::Arc::new(move |moved: Moved| told(&window, moved))
                    },
                    Shipped {
                        binary,
                        plugin,
                        shims,
                        git_hooks,
                    },
                    // Resolved once, here, like the plane: it is an environment ladder, and a
                    // second reader of it is a second answer to where this machine's store is.
                    charter_core::machine::config_root(),
                )
                // The host's event log (FD-9): one event per hook call, from every project.
                .recording_events(events())
                // A chat a handoff opened goes to the window, which files it on its workspace's
                // strip without taking the front (`handoff::Arrived`).
                .telling_arrivals({
                    let window = app.handle().clone();
                    std::sync::Arc::new(move |arrived: handoff::Arrived| {
                        windows::emit_for_plane(
                            &window,
                            &arrived.plane.clone(),
                            handoff::ARRIVED,
                            &arrived,
                        );
                    })
                })
                // A chat's permission prompt, held on its hook: the window lists it in needs-you
                // and answers it there (HP-6).
                .telling_asks({
                    let window = app.handle().clone();
                    std::sync::Arc::new(move |told: asking::Asking| {
                        windows::emit_for_plane(&window, &told.plane.clone(), asking::EVENT, &told);
                    })
                })
                // A harness started by hand in a shell tab: the window draws a banner on that
                // tab, offering to open it as a chat (ADR 0062).
                .telling_by_hand({
                    let window = app.handle().clone();
                    std::sync::Arc::new(move |told: hooks::ByHand| {
                        windows::emit_for_plane(
                            &window,
                            &told.plane.clone(),
                            hooks::BY_HAND,
                            &told,
                        );
                    })
                })
                // A file a chat's tool touched, confined to its folder: the window marks it in
                // the tree for a few seconds (FM-6). In memory only (D-86a).
                .telling_touches({
                    let window = app.handle().clone();
                    std::sync::Arc::new(move |told: hooks::ChatTouching| {
                        windows::emit_for_plane(
                            &window,
                            &told.plane.clone(),
                            hooks::TOUCHING,
                            &told,
                        );
                    })
                })
                // Each step of a smart close: the window draws the tab wrapping up, and closes it
                // when its record lands (ADR 0064).
                .telling_smart_close({
                    let window = app.handle().clone();
                    std::sync::Arc::new(move |step: smartclose::SmartClosing| {
                        windows::emit_for_plane(
                            &window,
                            &step.plane.clone(),
                            smartclose::EVENT,
                            &step,
                        );
                    })
                })
                // The plane moved on disk — a todo closed in a terminal, a workspace another
                // chat made — and the window reads it again (charter-app#264).
                .telling_changes({
                    let window = app.handle().clone();
                    std::sync::Arc::new(move |plane: PlaneId, changes: planewatch::What| {
                        windows::emit_for_plane(
                            &window,
                            &plane,
                            planewatch::CHANGED,
                            &planewatch::PlaneChanged::of(plane.clone(), changes),
                        );
                    })
                })
                // A plane let go of: the watch stops listening to its clones (FD-11).
                .telling_released({
                    let app = app.handle().clone();
                    std::sync::Arc::new(move |root: &std::path::Path| {
                        if let Some(watch) = app.try_state::<branchwatch::BranchWatch>() {
                            watch.let_go_of_plane(root);
                        }
                    })
                })
                // Auto-save saved a plane: the extensions that hear it are told, as after the
                // Save button (charter-app#343).
                .telling_saves({
                    let app = app.handle().clone();
                    std::sync::Arc::new(move |plane: PlaneId, root: std::path::PathBuf| {
                        app.state::<heard::Heard>().tell(
                            &app,
                            plane,
                            root,
                            charter_core::extension::events::Event::PlaneSaved,
                        );
                    })
                }),
            );

            // The working directory, resolved ONCE, to decide which plane the first window
            // opens. Everything after this names its plane; nothing asks the working
            // directory again. A launch that finds no plane leaves the app holding none,
            // which is a state and not a failure.
            let launch = planes::at_launch(&app.state::<Planes>(), std::env::current_dir());
            // Whether this launch puts the last quit's window set back. Read from THIS
            // process's arguments, once: a second launch's `--no-restore` would be about a
            // restore that happened hours ago, so the single-instance closure never reaches
            // this. After `at_launch`, which is what learns whether this launch follows a
            // restart to update — and that one always restores (charter-app#251).
            app.manage(Restoring::after(
                std::env::args(),
                app.state::<Planes>().restarted_to_update(),
            ));
            app.manage(launch);
            reached("the record is back");
            // `charter stop --all` in a terminal is heard here (OV-1).
            killswitch::hear(app.handle());

            // Last, and never fatal. A tray is somewhere to put the window; the sessions
            // are the work. A desktop with no system tray at all — some Linux sessions, and
            // any headless one — must still get its chats back, so a tray that cannot be
            // built is reported and the app carries on without one. Quit still lives in the
            // menu, and closing the window still hides it.
            // The automatic half of updating: a timer that checks, never one that installs.
            // Off in a test build and a development build (`updates::watch` says why).
            updates::watch(app.handle());

            if let Err(why) = lifecycle::tray(app.handle()) {
                tracing::warn!(
                    "charter: no tray icon ({why}); the window is reached from the dock"
                );
            }
            Ok(())
        })
        .build(tauri_context!())
        .inspect(|_| {
            reached("built");
            // Past the part of a launch that has no window in it, so the thread watching for
            // a slow one has nothing left to say.
            let _ = built.send(());
        })
        .expect("error while building tauri application")
        // Tauri ends the process itself, which runs no destructor and waits for no thread, so
        // the sessions are ended here — otherwise their programs are left to the operating
        // system, and one that ignores a hangup outlives the app that started it.
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                // First, so a restart finds the lock free (`instance.rs`).
                instance::let_go_at_exit(app);
                // An extension's program still answering is killed with its whole process
                // group, so that nothing an extension was asked to run outlives the window
                // that asked (`charter_core::executor`).
                app.state::<views::Views>().stop_all();
                app.state::<heard::Heard>().stop_all();
                // Every plane, not "the" plane: each one writes its own record into itself
                // and ends its own sessions. A failure is not worth refusing to exit over —
                // the next launch of that plane reads no record and starts empty.
                // Letting go of every plane also saves each one whose auto-save is on, and gives
                // its push a few seconds (`Planes::let_go_of_every_plane`, ADR 0051).
                app.state::<Planes>().let_go_of_all();
                // A secret a vault's Copy put on the clipboard does not outlive the app: its
                // clear was waiting on a timer that ends here.
                app.state::<vaults::SystemClipboard>().clear_at_exit();
                // Last, once everything above has let go: the launch on the session bus the
                // operator asked for from the window's notice, if they did (`portal.rs`).
                if let Some(bus) = app.try_state::<portal::SessionBus>() {
                    portal::restart_if_asked(&bus);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store that remembers the plane at `root` with `pins`, in that order.
    fn store_pinning(root: &std::path::Path, pins: &[&str]) -> charter_core::machine::Store {
        let mut store = charter_core::machine::Store::default();
        store.remember(root, 1);
        for pin in pins {
            store
                .pin_workspace(root, pin, true)
                .expect("the pin is written");
        }
        store
    }

    #[test]
    fn a_pin_whose_workspace_is_gone_is_named_with_its_place_in_the_order() {
        let plane = tempfile::tempdir().expect("a plane");
        for name in ["beta", "gamma"] {
            std::fs::create_dir_all(plane.path().join("workspaces").join(name)).expect("made");
        }
        let store = store_pinning(plane.path(), &["beta", "was-here", "gamma"]);

        let pins = pins_in(&store, plane.path());

        assert_eq!(pins.workspaces, ["beta", "gamma"]);
        assert_eq!(pins.missing, ["was-here"]);
        assert_eq!(pins.order, ["beta", "was-here", "gamma"]);
        assert!(pins.certain, "a whole listing is a certain answer");
    }

    /// A plane with `beta` and `gamma` under `workspaces/`, and a store pinning `pins`.
    fn plane_pinning(pins: &[&str]) -> (tempfile::TempDir, charter_core::machine::Store) {
        let plane = tempfile::tempdir().expect("a plane");
        for name in ["beta", "gamma"] {
            std::fs::create_dir_all(plane.path().join("workspaces").join(name)).expect("made");
        }
        let store = store_pinning(plane.path(), pins);
        (plane, store)
    }

    #[test]
    fn no_pin_is_named_gone_when_the_workspaces_directory_is_not_there() {
        let (plane, store) = plane_pinning(&["beta", "gamma"]);
        std::fs::remove_dir_all(plane.path().join("workspaces")).expect("removed");

        let pins = pins_in(&store, plane.path());
        assert_eq!(pins.missing, Vec::<String>::new());
        assert!(!pins.certain, "no listing is no answer about what has gone");
    }

    #[test]
    fn no_pin_is_named_gone_when_the_plane_root_is_not_there() {
        let (plane, store) = plane_pinning(&["beta", "gamma"]);
        let root = plane.path().to_path_buf();
        drop(plane);

        let pins = pins_in(&store, &root);

        assert_eq!(pins.missing, Vec::<String>::new());
        assert_eq!(pins.order, ["beta", "gamma"]);
        assert!(!pins.certain);
    }

    #[cfg(unix)]
    #[test]
    fn no_pin_is_named_gone_when_the_workspaces_directory_cannot_be_read() {
        use std::os::unix::fs::PermissionsExt;
        let (plane, store) = plane_pinning(&["beta", "was-here"]);
        let under = plane.path().join("workspaces");
        std::fs::set_permissions(&under, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        // Root reads anything, so on a runner that is root there is nothing to refuse.
        let refused = std::fs::read_dir(&under).is_err();

        let pins = pins_in(&store, plane.path());

        std::fs::set_permissions(&under, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        if refused {
            assert_eq!(pins.missing, Vec::<String>::new());
            assert!(!pins.certain);
        }
    }

    #[test]
    fn neither_name_of_a_rename_in_progress_is_named_gone() {
        // `wscmd::rename` moves the directory first and the pins last, so in between the old
        // name's pin names nothing: its journal says the rename is not done.
        let (plane, store) = plane_pinning(&["beta", "old-name", "new-name"]);
        let journal = charter_core::wscmd::rename::journal_path(plane.path());
        std::fs::create_dir_all(journal.parent().expect("a parent")).expect("made");
        std::fs::write(
            &journal,
            r#"{"from":"old-name","to":"new-name","moved":true}"#,
        )
        .expect("written");

        let pins = pins_in(&store, plane.path());
        assert_eq!(pins.missing, Vec::<String>::new());
        assert!(!pins.certain, "a rename between its steps is not an answer");

        std::fs::remove_file(&journal).expect("removed");
        let pins = pins_in(&store, plane.path());
        assert_eq!(pins.missing, ["old-name", "new-name"]);
        assert!(pins.certain);
    }

    #[cfg(unix)]
    #[test]
    fn a_workspace_whose_symlink_target_is_missing_is_not_named_gone() {
        // D-NO1-9: an unmounted target comes back, so the pin is not charter's to drop.
        let (plane, store) = plane_pinning(&["beta", "mounted", "was-here"]);
        std::os::unix::fs::symlink(
            plane.path().join("not-mounted"),
            plane.path().join("workspaces").join("mounted"),
        )
        .expect("linked");

        let pins = pins_in(&store, plane.path());
        assert_eq!(pins.missing, ["was-here"]);
        assert!(
            !pins.certain,
            "a link whose target is away is not known gone or there"
        );
    }

    #[test]
    fn no_pin_is_named_gone_when_the_workspaces_cannot_be_listed() {
        // The window offers to forget what `missing` names (V91c), so a listing that failed
        // must not read as every workspace having gone.
        let plane = tempfile::tempdir().expect("a plane");
        std::fs::write(plane.path().join("workspaces"), "not a directory").expect("written");
        let store = store_pinning(plane.path(), &["beta", "gamma"]);

        let pins = pins_in(&store, plane.path());

        assert_eq!(pins.missing, Vec::<String>::new());
        assert_eq!(pins.order, ["beta", "gamma"]);
        assert!(!pins.certain);
    }

    /// Writes `BINDINGS`, for when the commands above change:
    /// `cargo test -p charter-app -- --ignored`.
    #[test]
    #[ignore = "writes the bindings instead of checking them"]
    fn regenerate_the_typescript_the_ui_imports() {
        commands()
            .export(typescript(), BINDINGS)
            .expect("the bindings are written");
        std::fs::write(UI_RPC, ui_rpc_client()).expect("the UI RPC's client is written");
    }

    /// The bundled plugin's `hooks/hooks.json`, in the repository.
    fn hooks_file() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(PLUGIN_DIR)
            .join(charter_core::plugin::HOOKS_FILE)
    }

    /// Writes the bundled plugin's `hooks.json` from `charter_core::plugin::HOOKS`, for when
    /// the registry changes: `cargo test -p charter-app -- --ignored`.
    #[test]
    #[ignore = "writes the plugin's hooks file instead of checking it"]
    fn regenerate_the_bundled_plugins_hooks() {
        std::fs::write(hooks_file(), charter_core::plugin::hooks_json())
            .expect("the hooks file is written");
    }

    #[test]
    fn the_bundled_plugins_hooks_are_the_ones_the_registry_generates() {
        // One registry, and the file a chat loads is generated from it — never edited by hand,
        // so a hook cannot be wired that `charter hook` does not answer.
        assert_eq!(
            std::fs::read_to_string(hooks_file()).unwrap_or_default(),
            charter_core::plugin::hooks_json(),
            "{} is out of date: run `cargo test -p charter-app -- --ignored`",
            hooks_file().display()
        );
    }

    /// The bundled opencode shim, in the repository.
    fn opencode_shim_file() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(PLUGIN_DIR)
            .join(charter_core::opencode::SHIM_IN_BUNDLE)
    }

    /// Writes the bundled opencode shim from `charter_core::opencode`, for when it changes:
    /// `cargo test -p charter-app -- --ignored`.
    #[test]
    #[ignore = "writes the opencode shim instead of checking it"]
    fn regenerate_the_bundled_opencode_shim() {
        std::fs::write(
            opencode_shim_file(),
            charter_core::opencode::shim(charter_core::opencode::Arming::Session),
        )
        .expect("the shim is written");
    }

    #[test]
    fn the_bundled_opencode_shim_is_the_one_the_core_generates() {
        // One source, as for the hooks file: the routing and the words are the core's, so a
        // tool cannot be sent to a word `charter hook` does not answer (#371).
        assert_eq!(
            std::fs::read_to_string(opencode_shim_file()).unwrap_or_default(),
            charter_core::opencode::shim(charter_core::opencode::Arming::Session),
            "{} is out of date: run `cargo test -p charter-app -- --ignored`",
            opencode_shim_file().display()
        );
    }

    #[test]
    fn the_bundled_plugin_is_called_what_the_app_loads_it_as() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(PLUGIN_DIR)
            .join(".claude-plugin/plugin.json");
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(manifest).expect("plugin.json"))
                .expect("plugin.json is JSON");
        assert_eq!(doc["name"], charter_core::plugin::NAME);
    }

    #[test]
    fn nothing_the_plugin_ships_names_a_skill_by_the_plugins_old_name() {
        // #406: the plugin was `charter-app` until it was renamed `charter`, and a skill named
        // `charter-app:<skill>` is one no chat the app starts has any more.
        fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("a directory") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PLUGIN_DIR),
            &mut files,
        );
        assert!(!files.is_empty());
        for file in files {
            let text = std::fs::read_to_string(&file).expect("text");
            assert!(!text.contains("charter-app:"), "{}", file.display());
        }
    }

    #[test]
    fn the_bundle_carries_the_plugin_as_a_resource() {
        // `bundled_plugin` looks for it in the resource directory, so a build that did not
        // copy it there would start every chat unarmed.
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json is JSON");
        assert_eq!(
            conf["bundle"]["resources"][format!("{PLUGIN_DIR}/")],
            format!("{PLUGIN_DIR}/")
        );
    }

    #[test]
    fn the_window_is_never_narrower_than_the_title_bar_is_built_for() {
        // ADR 0054, amended 2026-09-26 (charter#403): 1024 px is the narrowest window, and the
        // title bar is held to room for two project tabs there (`title-bar.e2e.ts`). A window
        // the operator could drag narrower would be one nothing promises anything about.
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json is JSON");
        let main = conf["app"]["windows"]
            .as_array()
            .and_then(|windows| windows.iter().find(|one| one["label"] == lifecycle::WINDOW))
            .expect("tauri.conf.json declares the main window");
        assert_eq!(main["minWidth"], 1024);
    }

    #[test]
    fn the_typescript_the_ui_imports_is_the_one_these_commands_generate() {
        let out = tempfile::tempdir().expect("a directory to generate into");
        let generated = out.path().join("bindings.ts");
        commands()
            .export(typescript(), &generated)
            .expect("the bindings are generated");

        assert_eq!(
            std::fs::read_to_string(BINDINGS).unwrap_or_default(),
            std::fs::read_to_string(&generated).expect("the generated bindings are readable"),
            "{BINDINGS} is out of date: run `cargo test -p charter-app -- --ignored`"
        );
    }

    #[test]
    fn the_ui_rpcs_typescript_client_is_the_one_these_commands_generate() {
        assert_eq!(
            std::fs::read_to_string(UI_RPC).unwrap_or_default(),
            ui_rpc_client(),
            "{UI_RPC} is out of date: run `cargo test -p charter-app -- --ignored`"
        );
    }

    #[test]
    fn the_ui_rpcs_client_calls_every_command_over_the_link_and_none_over_tauri() {
        let client = ui_rpc_client();
        assert!(client.contains(r#"from "./uiRpcLink";"#));
        assert!(
            !client.contains("@tauri-apps/"),
            "the UI RPC's client imports nothing of Tauri's"
        );
        let (value_free, vault_values) = app_commands!(command_names);
        for command in value_free.iter().chain(vault_values) {
            let called = client.contains(&format!("(\"{command}\""));
            if TAKES_A_CHANNEL.contains(command) {
                assert!(
                    !called,
                    "`{command}` takes a channel, which a view carries instead"
                );
            } else if charter_session_protocol::ui::WINDOW_ONLY.contains(command) {
                assert!(!called, "`{command}` is the window's alone (HP-6)");
            } else {
                assert!(
                    called,
                    "`{command}` is in ipc_commands.rs and not in the UI RPC's client"
                );
            }
        }
    }

    #[test]
    fn the_ui_rpcs_client_has_no_command_that_takes_a_channel() {
        // A channel is Tauri's stream to the window; on the link, a terminal's bytes are a
        // view of the session protocol (ADR 0068 §4), so such a command has no UI RPC form.
        let client = ui_rpc_client();
        assert!(
            !client.contains("Channel<"),
            "a command that takes a channel is in the client"
        );
        let bindings = std::fs::read_to_string(BINDINGS).unwrap();
        for command in TAKES_A_CHANNEL {
            assert!(
                bindings.contains(&format!("(\"{command}\"")),
                "`{command}` is no longer a command of the window's, so nothing need leave it out"
            );
        }
    }

    #[test]
    fn answering_an_ask_is_the_window_s_alone_and_never_in_the_link_s_client() {
        // HP-6, V16, V75: the window's Tauri IPC answers an ask. On the link to `charterd` an
        // answer is the session protocol's `answer`, checked by FD-27's scope table, never a
        // second route through the UI RPC.
        let bindings = std::fs::read_to_string(BINDINGS).unwrap();
        assert!(bindings.contains("(\"answer_ask\""), "the window answers");
        assert!(!ui_rpc_client().contains("(\"answer_ask\""));
    }

    /// The window's commands that take a channel: `watch_session`, the one that streams a
    /// terminal to a pane.
    const TAKES_A_CHANNEL: &[&str] = &["watch_session"];
}

/// The bounded reader every automatic read of a branch goes through (FM-4, D-88h): this binary,
/// started again as the reader. In a test, this test binary, run again picking
/// [`reader_child`](tests_reader::reader_child).
pub(crate) fn reader() -> charter_core::files::Reader {
    #[cfg(test)]
    {
        charter_core::files::Reader::new(
            std::env::current_exe().unwrap_or_default(),
            [
                "tests_reader::reader_child",
                "--exact",
                "--nocapture",
                "--test-threads=1",
                charter_core::files::READ_ARG,
            ]
            .map(std::ffi::OsString::from),
        )
    }
    #[cfg(not(test))]
    {
        charter_core::files::Reader::this_binary()
            .unwrap_or_else(|_| charter_core::files::Reader::new(std::path::PathBuf::new(), []))
    }
}

#[cfg(test)]
mod tests_reader {
    /// The reader's child, in a run of this test binary that [`super::reader`] started: it
    /// answers the one question asked and exits. In any other run it does nothing.
    #[test]
    fn reader_child() {
        if let Some(code) = charter_core::files::serve_if_asked() {
            std::process::exit(code);
        }
    }
}
