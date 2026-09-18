//! The app's Rust side: what the UI can ask the core to do, as commands generated into
//! TypeScript by `tauri-specta`, so no shape is written by hand on either side.

mod chats;
mod lifecycle;
mod sessions;

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Instant;

use charter_core::engine::Size;
use charter_core::harness::Harness;
use charter_core::reopen::{self, Chat, Fresh, Reopened};
use tauri::Manager;
use tauri::ipc::Channel;
use tauri_specta::{Builder, collect_commands};

use chats::Chats;
use lifecycle::Quitting;

/// The size a session starts at. The pane it lands in tells it the real one at once, and a
/// chat put back at a launch has no pane yet to ask.
const STARTING: Size = Size {
    columns: 80,
    rows: 24,
};

/// The plane the app is running in, where there is one. Held because quitting has to write
/// the record into it, and by then the current directory is not worth trusting.
struct Plane(Option<PathBuf>);

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

/// The window says its first frame is on screen, which is where cold start ends.
///
/// It is silent unless `CHARTER_BENCH_LOG` is set — only `tools/bench.mjs` sets it — so in
/// an ordinary run this is one IPC call at startup that does nothing.
#[tauri::command]
#[specta::specta]
fn first_frame() {
    if std::env::var_os("CHARTER_BENCH_LOG").is_some() {
        println!(
            "charter-bench first-frame {}",
            STARTED.elapsed().as_millis()
        );
    }
}

/// The plane the app was started in, or why there is none.
#[tauri::command]
#[specta::specta]
fn plane_root() -> Result<String, String> {
    let cwd = std::env::current_dir()
        .map_err(|err| format!("cannot read the current directory: {err}"))?;
    charter_core::plane::resolve(&cwd)
        .map(|root| root.display().to_string())
        .map_err(|err| err.to_string())
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
fn plane_sidebar(chats: tauri::State<'_, Chats>) -> Result<Sidebar, String> {
    let cwd = std::env::current_dir()
        .map_err(|err| format!("cannot read the current directory: {err}"))?;
    let root = charter_core::plane::resolve(&cwd).map_err(|err| err.to_string())?;
    let plane = charter_core::workspaces::Plane::open(&root);

    let mut filed: std::collections::HashMap<String, Vec<OpenChat>> =
        std::collections::HashMap::new();
    let mut unfiled = Vec::new();
    for chat in chats.open_now().into_iter().map(OpenChat::from) {
        match chat
            .cwd
            .as_deref()
            .and_then(|c| plane.workspace_of(std::path::Path::new(c)))
        {
            Some(name) => filed.entry(name).or_default().push(chat),
            None => unfiled.push(chat),
        }
    }

    let mut workspaces = Vec::new();
    for name in plane.workspaces().map_err(|err| err.to_string())? {
        // A name off disk is re-checked before it is joined onto a path; one that cannot be
        // a workspace is left out rather than drawn.
        let Ok(ws) = plane.workspace(&name) else {
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
        personas: plane.personas().map_err(|err| err.to_string())?,
        persona: plane.default_persona(),
        unfiled,
    })
}

/// Starts a session, and remembers it as a chat so a quit can write it down. No program is
/// the operator's shell.
#[tauri::command]
#[specta::specta]
fn open_session(
    chats: tauri::State<'_, Chats>,
    program: Option<String>,
    args: Vec<String>,
    cwd: Option<String>,
    name: String,
    columns: u16,
    rows: u16,
) -> Result<u32, String> {
    chats.start(
        &Chat {
            program: program.unwrap_or_else(sessions::shell),
            args,
            cwd: cwd.map(PathBuf::from),
            name,
            resume: None,
            active: false,
        },
        Size { columns, rows },
    )
}

/// Ends a session and everything it started. It is no longer a chat a quit would record.
#[tauri::command]
#[specta::specta]
fn close_session(chats: tauri::State<'_, Chats>, session: u32) -> Result<(), String> {
    chats.close(session)
}

/// The chats the app already has open — at a launch, the ones put back from the record.
///
/// The window asks this instead of opening its own: putting the record back happens before
/// there is a window, so that a relaunch does not depend on a webview having run.
#[tauri::command]
#[specta::specta]
fn opened_chats(chats: tauri::State<'_, Chats>) -> Vec<OpenChat> {
    chats.open_now().into_iter().map(OpenChat::from).collect()
}

/// The chats this launch could not start, by name and reason. They are still recorded, and
/// will be tried again at the next launch.
#[tauri::command]
#[specta::specta]
fn chats_that_would_not_start(chats: tauri::State<'_, Chats>) -> Vec<(String, String)> {
    chats.would_not_start()
}

/// Says which chat is in front, so the record brings that one back in front.
#[tauri::command]
#[specta::specta]
fn chat_in_front(chats: tauri::State<'_, Chats>, session: Option<u32>) {
    chats.bring_to_front(session);
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
            in_front: open.in_front,
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

/// Sends what a pane typed to the session's program.
#[tauri::command]
#[specta::specta]
fn send_input(chats: tauri::State<'_, Chats>, session: u32, text: String) -> Result<(), String> {
    chats.sessions().input(session, &text)
}

/// Tells a session how big the pane showing it now is.
#[tauri::command]
#[specta::specta]
fn resize_session(
    chats: tauri::State<'_, Chats>,
    session: u32,
    columns: u16,
    rows: u16,
) -> Result<(), String> {
    chats.sessions().resize(session, Size { columns, rows })
}

/// Opens a view of a session for a pane that is now on screen: the channel is sent the screen
/// as it already is, and then the session's output. Answers with the id that closes the view.
#[tauri::command]
#[specta::specta]
fn watch_session(
    chats: tauri::State<'_, Chats>,
    session: u32,
    output: Channel<String>,
) -> Result<Watching, String> {
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
    })
}

/// Closes a view, for a pane that has gone off screen. The session keeps running.
#[tauri::command]
#[specta::specta]
fn unwatch_session(chats: tauri::State<'_, Chats>, session: u32, view: u32) -> Result<(), String> {
    chats.sessions().unwatch(session, view)
}

/// The sessions that are running, in the order they were opened.
#[tauri::command]
#[specta::specta]
fn running_sessions(chats: tauri::State<'_, Chats>) -> Vec<u32> {
    chats.sessions().running()
}

/// Every command the UI can call, in one place: the source of both the handler and the
/// TypeScript the UI imports.
fn commands() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        first_frame,
        plane_root,
        open_session,
        close_session,
        send_input,
        resize_session,
        watch_session,
        unwatch_session,
        running_sessions,
        opened_chats,
        chats_that_would_not_start,
        chat_in_front,
        ask_to_quit,
        quit,
        quit_cancelled,
        hide_window,
        window_showing,
        plane_sidebar,
    ])
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
    reached("run() entered");
    let commands = commands();

    #[cfg(debug_assertions)]
    commands
        .export(typescript(), BINDINGS)
        .expect("the TypeScript bindings are written");
    reached("the bindings are written");

    reached("building");
    let app = tauri::Builder::default()
        // First, so a second launch is handed to the app already running rather than
        // starting a second one — which would be a second set of sessions on the same plane.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            lifecycle::show(app);
        }))
        .plugin(tauri_plugin_opener::init());

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
            app.manage(Quitting::default());

            let plane = std::env::current_dir()
                .ok()
                .and_then(|cwd| charter_core::plane::find_root(&cwd).ok());
            // The record is written as what is open changes, and not only on the way out:
            // an app that is killed, or crashes, runs no exit handler, and a day's chats
            // would go with it. Outside a plane there is nowhere to write it, and the app
            // still runs — it just cannot bring anything back next time.
            let writing_to = plane.clone();
            let chats = Chats::recorded_by(Box::new(move |record| {
                if let Some(root) = &writing_to {
                    // A record that cannot be written is not worth interrupting the
                    // operator over, but it is worth saying: a refused record means every
                    // later launch comes back empty, and silence makes that look like a
                    // plane that never had chats in it.
                    if let Err(why) = reopen::write(root, record) {
                        eprintln!("charter: what is open was not recorded ({why})");
                    }
                }
            }));
            // Put back what was open before there is a window, so a relaunch does not
            // depend on a webview having run. The window asks `opened_chats` for the result.
            //
            // What happened is said out loud, on stderr. An operator who launched charter
            // somewhere without a plane, or whose chats did not come back, otherwise has
            // nothing at all to look at — and neither does a CI log.
            match &plane {
                Some(root) => {
                    let record = match reopen::read_or_refusal(root) {
                        Ok(record) => record,
                        Err(why) => {
                            // Not the same thing as an empty plane, and an operator told
                            // "nothing to reopen" would go looking in the wrong place.
                            eprintln!(
                                "charter: the record of what was open was refused ({why}); \
                                 nothing is reopened and nothing will be recorded until it is repaired"
                            );
                            reopen::Record::default()
                        }
                    };
                    let wanted = record.chats.len();
                    let back = chats.put_back(&record, STARTING).len();
                    if wanted > 0 {
                        eprintln!(
                            "charter: plane {}, {back} of {wanted} chats back",
                            root.display()
                        );
                        for (name, why) in chats.would_not_start() {
                            eprintln!(
                                "charter: {name} did not start ({why}); it is still recorded"
                            );
                        }
                    } else {
                        eprintln!("charter: plane {}, nothing to reopen", root.display());
                    }
                }
                None => eprintln!(
                    "charter: no plane here, so nothing is reopened and nothing is recorded \
                     (a plane is the nearest directory at or above this one with a charter.toml)"
                ),
            }
            reached("the record is back");
            app.manage(chats);
            app.manage(Plane(plane));

            // Last, and never fatal. A tray is somewhere to put the window; the sessions
            // are the work. A desktop with no system tray at all — some Linux sessions, and
            // any headless one — must still get its chats back, so a tray that cannot be
            // built is reported and the app carries on without one. Quit still lives in the
            // menu, and closing the window still hides it.
            if let Err(why) = lifecycle::tray(app.handle()) {
                eprintln!("charter: no tray icon ({why}); the window is reached from the dock");
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .inspect(|_| reached("built"))
        .expect("error while building tauri application")
        // Tauri ends the process itself, which runs no destructor and waits for no thread, so
        // the sessions are ended here — otherwise their programs are left to the operating
        // system, and one that ignores a hangup outlives the app that started it.
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let chats = app.state::<Chats>();
                // Written before the sessions are ended, because ending them is what makes
                // there be nothing to write. A failure here is not worth refusing to exit
                // over: the next launch reads no record and starts empty.
                if let Plane(Some(root)) = &*app.state::<Plane>()
                    && let Err(why) = reopen::write(root, &chats.record())
                {
                    eprintln!("charter: what was open was not recorded ({why})");
                }
                chats.end_all();
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
