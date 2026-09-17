//! The app's Rust side: what the UI can ask the core to do, as commands generated into
//! TypeScript by `tauri-specta`, so no shape is written by hand on either side.

mod sessions;

use charter_core::engine::Size;
use tauri::Manager;
use tauri::ipc::Channel;
use tauri_specta::{Builder, collect_commands};

use sessions::{Opening, Sessions};

/// A view a pane has open, and the size the screen it opened on was drawn for. The pane sets
/// its terminal to that size before it draws the screen, so what was wrapped stays wrapped.
#[derive(serde::Serialize, specta::Type)]
struct Watching {
    view: u32,
    columns: u16,
    rows: u16,
}

/// The plane the app was started in, or why there is none.
#[tauri::command]
#[specta::specta]
fn plane_root() -> Result<String, String> {
    let cwd = std::env::current_dir()
        .map_err(|err| format!("cannot read the current directory: {err}"))?;
    charter_core::plane::find_root(&cwd)
        .map(|root| root.display().to_string())
        .map_err(|err| err.to_string())
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
        plane_root,
        open_session,
        close_session,
        send_input,
        resize_session,
        watch_session,
        unwatch_session,
        running_sessions,
    ])
}

/// Where the generated TypeScript lives, relative to this crate.
const BINDINGS: &str = "../src/bindings.ts";

/// How the TypeScript is written, so that the app and the test that guards it agree.
fn typescript() -> specta_typescript::Typescript {
    specta_typescript::Typescript::default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let commands = commands();

    #[cfg(debug_assertions)]
    commands
        .export(typescript(), BINDINGS)
        .expect("the TypeScript bindings are written");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(commands.invoke_handler())
        .setup(|app| {
            app.manage(Sessions::new());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
