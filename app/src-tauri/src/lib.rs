//! The app's Rust side: what the UI can ask the core to do, as commands generated into
//! TypeScript by `tauri-specta`, so no shape is written by hand on either side.

mod sessions;

use std::sync::LazyLock;
use std::time::Instant;

use charter_core::engine::Size;
use tauri::Manager;
use tauri::ipc::Channel;
use tauri_specta::{Builder, collect_commands};

use sessions::{Opening, Sessions};

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

/// One chat in the sidebar. A chat is the app's own — nothing in the plane records it — so
/// this is a running session, labelled by where it is working.
#[derive(serde::Serialize, specta::Type)]
struct Chat {
    session: u32,
    cwd: Option<String>,
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
    chats: Vec<Chat>,
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
    unfiled: Vec<Chat>,
}

/// The sidebar, read from the plane on disk every time it is asked for.
///
/// Read fresh rather than cached: the plane is a directory the operator also edits by hand
/// and another charter process writes, so a cache here would be a second answer to "what is
/// on disk" that nothing invalidates.
#[tauri::command]
#[specta::specta]
fn plane_sidebar(sessions: tauri::State<'_, Sessions>) -> Result<Sidebar, String> {
    let cwd = std::env::current_dir()
        .map_err(|err| format!("cannot read the current directory: {err}"))?;
    let root = charter_core::plane::resolve(&cwd).map_err(|err| err.to_string())?;
    let plane = charter_core::workspaces::Plane::open(&root);

    let mut filed: std::collections::HashMap<String, Vec<Chat>> = std::collections::HashMap::new();
    let mut unfiled = Vec::new();
    for (session, cwd) in sessions.started_in() {
        let chat = Chat {
            session,
            cwd: cwd.clone(),
        };
        match cwd
            .as_deref()
            .and_then(|c| plane.workspace_of(std::path::Path::new(c)))
        {
            Some(name) => filed.entry(name).or_default().push(chat),
            None => unfiled.push(chat),
        }
    }

    let mut workspaces = Vec::new();
    for name in plane.workspaces().map_err(|err| err.to_string())? {
        let ws = plane.workspace(&name);
        workspaces.push(SidebarWorkspace {
            path: ws.dir().display().to_string(),
            vision: ws.vision(),
            todos: ws
                .todos()
                .map_err(|err| err.to_string())?
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

/// Starts a session. No program is the operator's shell.
#[tauri::command]
#[specta::specta]
fn open_session(
    sessions: tauri::State<'_, Sessions>,
    program: Option<String>,
    args: Vec<String>,
    cwd: Option<String>,
    columns: u16,
    rows: u16,
) -> Result<u32, String> {
    sessions.open(&Opening {
        program,
        args,
        cwd,
        size: Size { columns, rows },
    })
}

/// Ends a session and everything it started.
#[tauri::command]
#[specta::specta]
fn close_session(sessions: tauri::State<'_, Sessions>, session: u32) -> Result<(), String> {
    sessions.close(session)
}

/// Sends what a pane typed to the session's program.
#[tauri::command]
#[specta::specta]
fn send_input(
    sessions: tauri::State<'_, Sessions>,
    session: u32,
    text: String,
) -> Result<(), String> {
    sessions.input(session, &text)
}

/// Tells a session how big the pane showing it now is.
#[tauri::command]
#[specta::specta]
fn resize_session(
    sessions: tauri::State<'_, Sessions>,
    session: u32,
    columns: u16,
    rows: u16,
) -> Result<(), String> {
    sessions.resize(session, Size { columns, rows })
}

/// Opens a view of a session for a pane that is now on screen: the channel is sent the screen
/// as it already is, and then the session's output. Answers with the id that closes the view.
#[tauri::command]
#[specta::specta]
fn watch_session(
    sessions: tauri::State<'_, Sessions>,
    session: u32,
    output: Channel<String>,
) -> Result<Watching, String> {
    let watching = sessions.watch(
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
    sessions: tauri::State<'_, Sessions>,
    session: u32,
    view: u32,
) -> Result<(), String> {
    sessions.unwatch(session, view)
}

/// The sessions that are running, in the order they were opened.
#[tauri::command]
#[specta::specta]
fn running_sessions(sessions: tauri::State<'_, Sessions>) -> Vec<u32> {
    sessions.running()
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
        plane_sidebar,
    ])
}

/// Where the generated TypeScript lives.
///
/// Anchored to this crate's directory rather than written relative, because a debug build
/// exports it on startup and the process's working directory is not this crate's: the
/// scenario tests launch the app from `app/`, and a relative path wrote a stray
/// `src/bindings.ts` at the repository root on every run.
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
    let commands = commands();

    #[cfg(debug_assertions)]
    commands
        .export(typescript(), BINDINGS)
        .expect("the TypeScript bindings are written");

    let app = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    // What the scenario tests drive the window through. The feature is off in every build
    // anyone is given, so nothing here can be reached in one.
    #[cfg(feature = "e2e")]
    let app = app
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    app.invoke_handler(commands.invoke_handler())
        .setup(|app| {
            app.manage(Sessions::new());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // Tauri ends the process itself, which runs no destructor and waits for no thread, so
        // the sessions are ended here — otherwise their programs are left to the operating
        // system, and one that ignores a hangup outlives the app that started it.
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<Sessions>().end_all();
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
