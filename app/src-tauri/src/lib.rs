//! The app's Rust side: what the UI can ask the core to do, as commands generated into
//! TypeScript by `tauri-specta`, so no shape is written by hand on either side.

mod alerts;
mod chats;
mod doctor;
mod extensions;
mod hooks;
mod lifecycle;
mod opener;
mod panels;
mod panics;
mod pin;
mod planes;
mod sessions;
mod slowstart;
mod updates;
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

/// The `charter` binary a hook runs, or none when the app cannot find one.
///
/// Its own directory first, because the app and the binary are built and shipped together —
/// the one on `PATH` may be an older install, or the Python charter, and a hook pointed at
/// either would be answering a different program's idea of these events. `CHARTER_BINARY`
/// overrides it, which is how a scenario test points the hooks at the binary it just built.
///
/// It must EXIST: arming a hook at a path that is not there would put an error in the
/// harness's log on every single event, which is worse than the chats reading `unknown`.
pub(crate) fn charter_binary() -> Option<PathBuf> {
    let named = std::env::var_os("CHARTER_BINARY").map(PathBuf::from);
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("charter")));
    named.into_iter().chain(beside).find(|path| path.is_file())
}

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
    let _ = app.emit("chat-moved", &moved);
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
fn already_looking_at(app: &tauri::AppHandle, moved: &Moved) -> bool {
    let Some(window) = app.get_webview_window("main") else {
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
    let _ = app.emit(SECOND_LAUNCH, cwd);
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
    if std::env::var_os("CHARTER_LAUNCH_LOG").is_some() {
        eprintln!(
            "charter-launch {:>5} ms  {step}",
            STARTED.elapsed().as_millis()
        );
    }
}

/// Whether the first frame has been reported, so a webview reload is not a second launch.
static FIRST_FRAME: AtomicBool = AtomicBool::new(false);

/// The window says its first frame is on screen, which is where cold start ends.
///
/// It answers with the one line to put on screen when that took longer than the limit, and
/// with nothing when it did not. An operator who launched charter from an icon has no
/// standard error to read, and a start that took half a minute with no window has to say why
/// somewhere they can see it (charter-app#24). Only the first call is answered: a webview
/// that reloads has not started the process again.
///
/// `CHARTER_BENCH_LOG` — which only `tools/bench.mjs` sets — also prints the number here.
#[tauri::command]
#[specta::specta]
fn first_frame() -> Option<String> {
    let took = STARTED.elapsed();
    if std::env::var_os("CHARTER_BENCH_LOG").is_some() {
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
    /// The harness profile it started on, where it started on one.
    profile: Option<String>,
    /// The persona it adopted.
    persona: Option<String>,
    /// What its harness cannot tell charter, said on the chat — none where it tells all.
    unreported: Option<String>,
    /// Whether the operator pinned it (charter ADR 0039). It rides the plane's own app
    /// record, so a pinned chat comes back pinned at the next launch.
    pinned: bool,
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

/// The sidebar, read from the plane on disk every time it is asked for.
///
/// Read fresh rather than cached: the plane is a directory the operator also edits by hand
/// and another charter process writes, so a cache here would be a second answer to "what is
/// on disk" that nothing invalidates.
///
/// The chats are the ones `Chats` already holds — one model of a chat, not a second derived
/// from the sessions. What files one under a workspace is the directory it works in, because
/// nothing on the plane records a chat: `.charter/frame/` belongs to the tmux frame and the
/// app stays out of it.
#[tauri::command]
#[specta::specta]
fn plane_sidebar(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Sidebar, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    let on_disk = charter_core::workspaces::Plane::open(root);

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

    let mut workspaces = Vec::new();
    for name in on_disk.workspaces().map_err(|err| err.to_string())? {
        // A name off disk is re-checked before it is joined onto a path; one that cannot be
        // a workspace is left out rather than drawn.
        let Ok(ws) = on_disk.workspace(&name) else {
            continue;
        };
        workspaces.push(SidebarWorkspace {
            path: ws.dir().display().to_string(),
            vision: ws.vision(),
            // A store charter cannot read costs that workspace its todo list, not the
            // window its workspaces.
            todos: ws
                .todos()
                .unwrap_or_default()
                .into_iter()
                .map(|todo| todo.title)
                .collect(),
            chats: filed.remove(&name).unwrap_or_default(),
            name,
        });
    }

    Ok(Sidebar {
        root: root.display().to_string(),
        workspaces,
        personas: on_disk.personas().map_err(|err| err.to_string())?,
        persona: on_disk.default_persona(),
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
#[tauri::command]
#[specta::specta]
fn workspace_panels(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
) -> Result<panels::Panels, String> {
    panels::of(planes.held(&plane)?.root(), &workspace)
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
#[tauri::command]
#[specta::specta]
fn start_options(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<StartOptions, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    let (set, check) = charter_core::profiles::for_launch(root);
    let on_disk = charter_core::workspaces::Plane::open(root);
    Ok(StartOptions {
        profiles: set
            .profiles()
            .iter()
            .map(|p| ProfileRow {
                name: p.name.clone(),
                kind: p.kind.clone(),
                shown: charter_core::profiles::display(p),
                source: p.source.as_str().to_owned(),
                is_default: set.default.as_deref() == Some(p.name.as_str()),
                approval: charter_core::profiletrust::approval_needed(root, p)
                    .map(|a| a.as_str().to_owned()),
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
    let now = charter_core::profiles::display(profile);
    if now != shown {
        return Err(format!(
            "profile '{}' changed while you were reading it, so nothing was approved and \
             nothing was started. It now runs: {now}",
            charter_core::shown::short(&name)
        ));
    }
    charter_core::profiletrust::record_launched(
        root,
        &profile.name,
        &charter_core::profiletrust::fingerprint(profile),
    )
    .map_err(|err| {
        format!(
            "charter could not record that approval ({err}), so it will not \
                            start the profile — it would only ask again."
        )
    })
}

/// Starts a chat on a harness profile, with a persona.
///
/// A command of its own rather than a flag on `open_session`, so neither can be mistaken
/// for the other by a caller passing null: this one goes through every gate a launch has,
/// and that one opens the operator's shell.
///
/// `show_footer` is the picker's footer checkbox, and it is a property of THIS chat
/// (charter ADR 0029). It reaches the harness as an environment variable set at the exec, so
/// it is decided here and nowhere later: Claude Code's footer command inherits the
/// environment its harness was started with, and no later click can change it.
// One over clippy's threshold, and it is a command's argument list: every one of these is a
// separate value the window sends, and folding a few into a struct would put a generated
// TypeScript type between the picker and the call for nothing. Not a doc comment, because
// the generated bindings carry those and this is about the Rust.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
fn start_chat(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    profile: String,
    persona: Option<String>,
    cwd: Option<String>,
    name: String,
    show_footer: bool,
    columns: u16,
    rows: u16,
) -> Result<Started, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    let start = charter_core::start::Start {
        profile: Some(profile.clone()),
        persona: persona.clone(),
        name: name.clone(),
        cwd: cwd.as_deref().map(PathBuf::from),
        resume: None,
        show_footer,
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
    };
    let session = held
        .chats()
        .start_ready(&chat, &ready, Size { columns, rows })?;
    Ok(Started {
        session,
        wired: (!ready.wired.is_empty()).then_some(ready.wired),
    })
}

/// A chat that started: its session, and the one line to say if charter wired its profile
/// on the way.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
struct Started {
    session: u32,
    wired: Option<String>,
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
    let chat = Chat {
        program: program.unwrap_or_else(sessions::shell),
        args,
        cwd: cwd.map(PathBuf::from),
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
    };
    // The board already knows about it: `Chats` announces a chat BEFORE its program starts,
    // so its very first hook lands somewhere. Registering it here would be too late.
    planes
        .held(&plane)?
        .chats()
        .start(&chat, Size { columns, rows })
}

/// Ends a session and everything it started. It is no longer a chat a quit would record.
#[tauri::command]
#[specta::specta]
fn close_session(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<(), String> {
    let held = planes.held(&plane)?;
    held.chats().close(session)?;
    // Off the board entirely, not merely ended: a report that arrives for it afterwards —
    // from a hook that outlived the harness by a moment — moves nothing.
    held.hooks().board().closed(session);
    Ok(())
}

/// The chats the app already has open — at a launch, the ones put back from the record.
///
/// The window asks this instead of opening its own: putting the record back happens before
/// there is a window, so that a relaunch does not depend on a webview having run.
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

/// The chats this launch could not start, by name and reason. They are still recorded, and
/// will be tried again at the next launch.
#[tauri::command]
#[specta::specta]
fn chats_that_would_not_start(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<(String, String)>, String> {
    Ok(planes.held(&plane)?.chats().would_not_start())
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

/// What the operator has pinned in one project (charter ADR 0039, stored per ADR 0040).
///
/// The project's own pin and its pinned workspaces come from the machine store; a pinned
/// CHAT is not here, because a chat pin rides that chat's own record and reaches the window
/// on `OpenChat::pinned` with the chat it is about.
#[derive(serde::Serialize, specta::Type)]
struct Pins {
    /// Whether this project itself is pinned.
    project: bool,
    /// Its pinned workspaces that still exist, in the plane's own order.
    ///
    /// The plane's order and never the pin's: a pin says WHICH workspaces come first, not in
    /// what order they do, so two operators who pin the same two see the same arrangement.
    workspaces: Vec<String>,
    /// Pins that no longer name a workspace on the plane — renamed, or removed.
    ///
    /// **Named rather than dropped silently**, and never drawn as a workspace: a window that
    /// drew one would be offering a workspace the plane does not have. This is ADR 0034's
    /// own hazard for a trust entry keyed on a path, one scope down.
    missing: Vec<String>,
}

/// What this operator has pinned in this project.
#[tauri::command]
#[specta::specta]
fn plane_pins(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Pins, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    // The plane's own list, read off the disk the same way the sidebar is: what workspaces
    // exist is the plane's answer and never the store's, so a pin is only ever matched
    // against it.
    let there = charter_core::workspaces::Plane::open(&root)
        .workspaces()
        .unwrap_or_default();
    let names: Vec<&str> = there.iter().map(String::as_str).collect();
    let store = planes.remembered().store;
    let (kept, missing) = store.pinned_workspaces(&root, &names);
    Ok(Pins {
        project: store.recent(&root).is_some_and(|entry| entry.pinned),
        workspaces: kept.into_iter().map(str::to_owned).collect(),
        missing,
    })
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
            profile: open.profile,
            persona: open.persona,
            in_front: open.in_front,
            pinned: open.pinned,
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
        .map(|open| held.hooks().now(open.session))
        .collect())
}

/// Sends what a pane typed to the session's program.
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
        .chats()
        .sessions()
        .input(session, &text)
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
    let watching = planes.held(&plane)?.chats().sessions().watch(
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

/// Every command the UI can call, in one place: the source of both the handler and the
/// TypeScript the UI imports.
fn commands() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            first_frame,
            plane_at_launch,
            open_planes,
            close_plane,
            opener::recent_planes,
            opener::pick_project,
            opener::open_plane,
            opener::approve_plane,
            opener::planes_to_restore,
            opener::window_holds_planes,
            opener::create_project,
            open_session,
            close_session,
            send_input,
            resize_session,
            watch_session,
            unwatch_session,
            running_sessions,
            chat_states,
            opened_chats,
            chats_that_would_not_start,
            chat_in_front,
            plane_pins,
            pin_project,
            pin_workspace,
            pin_chat,
            ask_to_quit,
            quit,
            quit_cancelled,
            hide_window,
            window_showing,
            plane_sidebar,
            workspace_panels,
            workspaces::workspace_create,
            workspaces::workspace_at_risk,
            workspaces::workspace_remove,
            workspace_repos,
            alerts_everywhere,
            start_options,
            approve_profile,
            start_chat,
            worktrees::worktree_of_chat,
            worktrees::worktree_list,
            worktrees::worktree_remove,
            worktrees::worktree_merge,
            updates::update_channel,
            updates::set_update_channel,
            updates::check_for_update,
            updates::install_update,
            extensions::installed_extensions,
            extensions::pick_extension,
            extensions::install_extension,
            extensions::approve_extension,
            extensions::forget_extension,
            extensions::extension_themes,
            doctor::plane_doctor,
            pin::plane_pin,
        ])
        // What `update://checked` carries. It crosses on an event rather than a command, so it
        // is named here or the window would have to write the shape out by hand.
        .typ::<updates::Offer>()
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Read first, so that what it holds is when the process started and not when the window
    // first asked.
    LazyLock::force(&STARTED);
    // Before anything that can panic: a panic that ends the app is written down on its way
    // out, where one that went to a standard error nobody reads was lost (charter-app#16).
    panics::record();
    reached("run() entered");
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
    // on screen yet — there is no screen — so it is said on standard error, from a thread,
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
                &mut |line| eprintln!("{line}"),
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
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                lifecycle::hide_rather_than_close(window);
            }
        })
        .setup(|app| {
            reached("setup");
            // Where a panic is kept, now that the app can be told where its logs belong. An app
            // with no log directory still has standard error, which is all it had before.
            if let Ok(logs) = app.path().app_log_dir() {
                panics::keep_in(&logs);
            }
            app.manage(Quitting::default());
            // What each window is holding, and which of its projects it has in front. Empty
            // until a window says, and an empty answer means "not looking", so a notification
            // is sent rather than suppressed.
            app.manage(Showing::default());
            // Whether this launch puts the last quit's window set back. Read from THIS
            // process's arguments, once: a second launch's `--no-restore` would be about a
            // restore that happened hours ago, so the single-instance closure never reaches
            // this.
            app.manage(Restoring::from_args(std::env::args()));

            // Which `charter` a hook runs. Without one, nothing is armed and every chat
            // reads `unknown` — never a hook pointed at a path that is not there. It is a
            // property of this build and not of a project, so every plane arms with it.
            let binary = charter_binary();
            if binary.is_none() {
                eprintln!(
                    "charter: no `charter` binary beside the app, so no chat can report its \
                     state; every one will show as unknown"
                );
            }
            // The registry is managed BEFORE a plane is opened, because opening one starts
            // programs, and a program that dies at once tells the board, which tells the
            // window, which asks this registry what the chat is called.
            app.manage(Planes::telling(
                {
                    let window = app.handle().clone();
                    std::sync::Arc::new(move |moved: Moved| told(&window, moved))
                },
                binary,
                // Resolved once, here, like the plane: it is an environment ladder, and a
                // second reader of it is a second answer to where this machine's store is.
                charter_core::machine::config_root(),
            ));

            // The working directory, resolved ONCE, to decide which plane the first window
            // opens. Everything after this names its plane; nothing asks the working
            // directory again. A launch that finds no plane leaves the app holding none,
            // which is a state and not a failure.
            let launch = planes::at_launch(&app.state::<Planes>(), std::env::current_dir());
            app.manage(launch);
            reached("the record is back");

            // Last, and never fatal. A tray is somewhere to put the window; the sessions
            // are the work. A desktop with no system tray at all — some Linux sessions, and
            // any headless one — must still get its chats back, so a tray that cannot be
            // built is reported and the app carries on without one. Quit still lives in the
            // menu, and closing the window still hides it.
            // The automatic half of updating: a timer that checks, never one that installs.
            // Off in a test build and a development build (`updates::watch` says why).
            updates::watch(app.handle());

            if let Err(why) = lifecycle::tray(app.handle()) {
                eprintln!("charter: no tray icon ({why}); the window is reached from the dock");
            }
            Ok(())
        })
        .build(tauri::generate_context!())
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
                // Every plane, not "the" plane: each one writes its own record into itself
                // and ends its own sessions. A failure is not worth refusing to exit over —
                // the next launch of that plane reads no record and starts empty.
                app.state::<Planes>().let_go_of_all();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes `BINDINGS`, for when the commands above change:
    /// `cargo test -p charter-app -- --ignored`.
    #[test]
    #[ignore = "writes the bindings instead of checking them"]
    fn regenerate_the_typescript_the_ui_imports() {
        commands()
            .export(typescript(), BINDINGS)
            .expect("the bindings are written");
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
}
