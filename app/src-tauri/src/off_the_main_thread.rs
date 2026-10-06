//! Which commands answer off the thread that asked (SC-2, #680).
//!
//! Tauri runs a synchronous command on the thread that handed it the request, which in the app
//! is the main thread: the one that pumps the window's events. A command that walks the plane's
//! files there holds the window for as long as the walk takes, and a plane with dozens of
//! workspaces and thousands of memories makes that a hitch a person sees. So the commands that
//! read every workspace, every memory or every session record are `async` and do their reading
//! on a blocking thread, as `workspace_repos` already did for git.
//!
//! `chat_usage` stays synchronous too: it reads one file of sixteen rows, about 30 µs, and walks
//! nothing.
//!
//! A terminal's own commands stay synchronous on purpose. They take well under 5 ms, and the
//! window relies on their order: a pane's resize must land before the watch that follows it
//! (#891). Nothing in this module runs in the app; it is the tests that hold both halves.

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::mpsc;
    use std::thread::ThreadId;
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};
    use tauri::Manager;
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponse, InvokeResponseBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder};
    use tauri::webview::InvokeRequest;

    use crate::lifecycle::WINDOW;
    use crate::planes::{PlaneId, Planes};

    /// The app on Tauri's mock runtime with the commands under test, and a registry holding
    /// the plane at `root`.
    fn app_holding(root: &Path) -> (tauri::App<MockRuntime>, PlaneId) {
        let planes = Planes::telling(std::sync::Arc::new(|_| {}), crate::Shipped::default(), None);
        let plane = planes.open(root);
        let app = mock_builder()
            .manage(planes)
            .invoke_handler(tauri::generate_handler![
                crate::plane_sidebar,
                crate::workspace_panels,
                crate::plane_root_panels,
                crate::curation::curation_offers,
                crate::memories::memory_read,
                crate::memories::memory_edit,
                crate::memories::memory_archive,
                crate::memories::memory_unarchive,
                crate::memories::memory_archived,
                crate::memories::memory_create,
                crate::memories::memory_move,
                crate::memories::memory_scopes,
                crate::todos::todo_add,
                crate::todos::todo_done,
                crate::todos::todo_forget,
                crate::todos::todo_read,
                crate::personas::persona_create,
                crate::personas::persona_remove,
                crate::resize_session,
                crate::unwatch_session,
            ])
            .build(tauri_context!(test = true))
            .expect("the app builds");
        (app, plane)
    }

    /// What one invoke came to: the thread its answer was given on, how long the asking
    /// thread was held before it could go back to the window, and the answer.
    struct Asked {
        answered_on: ThreadId,
        held_for: Duration,
        answer: Result<Value, Value>,
    }

    /// Invokes `command` with `args` from the main window, as the app's page would, and waits
    /// for its answer.
    fn ask(app: &tauri::App<MockRuntime>, command: &str, args: Value) -> Asked {
        let window = app
            .get_webview_window(WINDOW)
            .expect("the main window is open");
        let (tx, rx) = mpsc::sync_channel(1);
        let asked_at = Instant::now();
        window.as_ref().clone().on_message(
            InvokeRequest {
                cmd: command.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().expect("a URL"),
                body: InvokeBody::Json(args),
                headers: tauri::http::HeaderMap::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
            Box::new(move |_, _, response, _, _| {
                let _ = tx.send((std::thread::current().id(), response));
            }),
        );
        let held_for = asked_at.elapsed();
        let (answered_on, response) = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("the command answered");
        let json = |body: InvokeResponseBody| match body {
            InvokeResponseBody::Json(text) => serde_json::from_str(&text).expect("JSON"),
            InvokeResponseBody::Raw(_) => Value::Null,
        };
        let answer = match response {
            InvokeResponse::Ok(body) => Ok(json(body)),
            InvokeResponse::Err(err) => Err(err.0),
        };
        Asked {
            answered_on,
            held_for,
            answer,
        }
    }

    fn app_with_its_window(root: &Path) -> (tauri::App<MockRuntime>, PlaneId) {
        let (app, plane) = app_holding(root);
        tauri::WebviewWindowBuilder::new(&app, WINDOW, tauri::WebviewUrl::default())
            .build()
            .expect("the main window");
        (app, plane)
    }

    /// A plane with the workspace `alpha`, a todo and a memory in it, and the shared store.
    fn a_plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("charter.toml");
        std::fs::create_dir_all(root.join("workspaces/alpha")).expect("alpha");
        std::fs::write(root.join("workspaces/alpha/workspace.md"), "# alpha\n").expect("its md");
        std::fs::create_dir_all(root.join("personas/_shared/memory")).expect("the shared store");
        dir
    }

    /// Asks `command` and says where it was answered, failing on a refusal: a command that
    /// refused at once would be answered wherever it refused, which is not what is under test.
    fn answered(app: &tauri::App<MockRuntime>, command: &str, args: Value) -> (Asked, Value) {
        let asked = ask(app, command, args);
        let answer = asked
            .answer
            .clone()
            .unwrap_or_else(|err| panic!("{command} refused: {err}"));
        (asked, answer)
    }

    #[test]
    fn the_commands_that_read_the_plane_answer_off_the_thread_that_asked() {
        let dir = a_plane();
        let (app, plane) = app_with_its_window(dir.path());
        let asking = std::thread::current().id();
        let alpha = json!({ "kind": "workspace", "name": "alpha" });

        let mut on_the_asking_thread = Vec::new();
        let mut check = |command: &str, args: Value| -> Value {
            let (asked, answer) = answered(&app, command, args);
            if asked.answered_on == asking {
                on_the_asking_thread.push(command.to_owned());
            }
            answer
        };

        check("plane_sidebar", json!({ "plane": plane }));
        check(
            "workspace_panels",
            json!({ "plane": plane, "workspace": "alpha" }),
        );
        check("plane_root_panels", json!({ "plane": plane }));
        check(
            "curation_offers",
            json!({ "plane": plane, "subjects": ["workspace:alpha", "plane"] }),
        );
        let made = check(
            "memory_create",
            json!({ "plane": plane, "scope": alpha, "title": "A fact", "text": "It holds." }),
        );
        let slug = made["slug"].as_str().expect("a slug").to_owned();
        let read = check(
            "memory_read",
            json!({ "plane": plane, "scope": alpha, "slug": slug }),
        );
        check(
            "memory_edit",
            json!({
                "plane": plane, "scope": alpha, "slug": slug, "title": "A fact",
                "text": "It still holds.", "read": read["text"], "overwrite": false,
            }),
        );
        let archived = check(
            "memory_archive",
            json!({ "plane": plane, "scope": alpha, "slug": slug }),
        );
        check("memory_archived", json!({ "plane": plane, "scope": alpha }));
        check(
            "memory_unarchive",
            json!({
                "plane": plane, "scope": alpha,
                "archived": archived["archived"], "restoreAs": null,
            }),
        );
        check("memory_scopes", json!({ "plane": plane }));
        check(
            "memory_move",
            json!({ "plane": plane, "scope": alpha, "slug": slug, "to": { "kind": "shared" } }),
        );

        // The window's writes tell the plane's model what they wrote, under the lock a change
        // nobody could name holds while every workspace is read again (FD-10c).
        let todos = dir.path().join("workspaces/alpha/todos");
        let slugs = || -> Vec<String> {
            let mut slugs: Vec<String> = std::fs::read_dir(&todos)
                .expect("the todo store")
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.strip_suffix(".md").map(str::to_owned)
                })
                // The store's index, which is not a todo.
                .filter(|slug| slug != "MEMORY")
                .collect();
            slugs.sort();
            slugs
        };
        check(
            "todo_add",
            json!({ "plane": plane, "workspace": "alpha", "text": "Ship it" }),
        );
        check(
            "todo_add",
            json!({ "plane": plane, "workspace": "alpha", "text": "Drop it" }),
        );
        let [drop, ship] = <[String; 2]>::try_from(slugs()).expect("two todos");
        check(
            "todo_read",
            json!({ "plane": plane, "workspace": "alpha", "slug": ship }),
        );
        check(
            "todo_done",
            json!({ "plane": plane, "workspace": "alpha", "slug": ship }),
        );
        check(
            "todo_forget",
            json!({ "plane": plane, "workspace": "alpha", "slug": drop }),
        );
        check(
            "persona_create",
            json!({
                "plane": plane, "name": "scribe", "role": null,
                "delegateWhen": "writing things down", "parent": null,
            }),
        );
        check(
            "persona_remove",
            json!({ "plane": plane, "name": "scribe" }),
        );

        assert!(
            on_the_asking_thread.is_empty(),
            "answered on the thread that asked, which in the app is the window's: \
             {on_the_asking_thread:?}"
        );
    }

    #[test]
    fn a_terminals_resize_and_unwatch_answer_on_the_thread_that_asked_so_they_keep_their_order() {
        // #891: a pane's resize must land before the watch after it, and Tauri keeps that order
        // only for synchronous commands. Both refuse here, as there is no session 7; where the
        // refusal is given is the point.
        let dir = a_plane();
        let (app, plane) = app_with_its_window(dir.path());
        let asking = std::thread::current().id();

        let resize = ask(
            &app,
            "resize_session",
            json!({ "plane": plane, "session": 7, "columns": 80, "rows": 24 }),
        );
        let unwatch = ask(
            &app,
            "unwatch_session",
            json!({ "plane": plane, "session": 7, "view": 1 }),
        );

        assert_eq!(resize.answered_on, asking);
        assert_eq!(unwatch.answered_on, asking);
    }

    /// A plane the size the research measured against: `workspaces` workspaces, each with
    /// `todos` todos, `memories` memories in the first, and `records` session records at the
    /// plane root.
    fn a_large_plane(
        workspaces: usize,
        todos: usize,
        memories: usize,
        records: usize,
    ) -> tempfile::TempDir {
        let dir = a_plane();
        let root = dir.path();
        let stamp: chrono::NaiveDateTime = "2026-10-02T09:00:00".parse().expect("a stamp");
        let plane = purlis_core::workspaces::Plane::open(root);
        for w in 0..workspaces {
            let name = format!("ws{w:03}");
            std::fs::create_dir_all(root.join("workspaces").join(&name)).expect("a workspace");
            std::fs::write(
                root.join("workspaces").join(&name).join("workspace.md"),
                format!("# {name}\n"),
            )
            .expect("its md");
            let ws = plane.workspace(&name).expect("a workspace name");
            for t in 0..todos {
                ws.add_todo(&format!("todo {t} of {name}"), stamp)
                    .expect("a todo");
            }
            if w == 0 {
                for m in 0..memories {
                    ws.remember(
                        &format!("memory {m} of {name}: a fact worth keeping"),
                        stamp,
                    )
                    .expect("a memory");
                }
            }
        }
        // And the plane root's session records, which its Sessions panel lists.
        std::fs::create_dir_all(root.join("sessions")).expect("sessions/");
        for r in 0..records {
            std::fs::write(
                root.join("sessions")
                    .join(format!("20261002-{:06}-record-{r}.md", r % 1_000_000)),
                format!("# Record {r}\n\n## Goal\n\nOne.\n\n## Done\n\nIt.\n"),
            )
            .expect("a record");
        }
        dir
    }

    /// How long each command holds the thread that asked, on a large plane. The numbers are
    /// for the PR, not a gate: `cargo test -p purlis-app off_the_main_thread -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore = "a measurement, printed; run it by hand"]
    fn how_long_each_command_holds_the_thread_that_asked() {
        let dir = a_large_plane(60, 10, 1000, 300);
        let (app, plane) = app_with_its_window(dir.path());
        let ws = json!({ "kind": "workspace", "name": "ws000" });
        let made = ask(
            &app,
            "memory_create",
            json!({ "plane": plane, "scope": ws, "title": "Read me", "text": "Here." }),
        )
        .answer
        .expect("a memory");
        let cases = [
            ("plane_sidebar", json!({ "plane": plane })),
            (
                "workspace_panels",
                json!({ "plane": plane, "workspace": "ws000" }),
            ),
            ("plane_root_panels", json!({ "plane": plane })),
            (
                "curation_offers",
                json!({ "plane": plane, "subjects": ["workspace:ws000", "plane"] }),
            ),
            (
                "memory_create",
                json!({ "plane": plane, "scope": ws, "title": "One more", "text": "Kept." }),
            ),
            (
                "memory_read",
                json!({ "plane": plane, "scope": ws, "slug": made["slug"] }),
            ),
        ];
        for (command, args) in cases {
            let mut held: Vec<Duration> = (0..5)
                .map(|_| ask(&app, command, args.clone()).held_for)
                .collect();
            held.sort();
            println!(
                "{command}: asking thread held for a median of {:?} (min {:?}, max {:?})",
                held[2], held[0], held[4]
            );
        }
        // And `chat_usage`'s read for an open chat, which stays synchronous: its usage file, a
        // ring of sixteen rows.
        let sessions = purlis_core::usage::sessions_dir(dir.path());
        std::fs::create_dir_all(&sessions).expect("its sessions");
        let rows: String = (0..16)
            .map(|n| format!("{},{},90,{}\n", 1000 * n, 100 * n, n))
            .collect();
        std::fs::write(sessions.join("a-conversation.usage"), rows).expect("a usage file");
        let started = Instant::now();
        for _ in 0..100 {
            let _ = crate::usage::of(dir.path(), "a-conversation");
        }
        println!(
            "chat_usage's read of an open chat's usage file: {:?} each",
            started.elapsed() / 100
        );
    }
}
