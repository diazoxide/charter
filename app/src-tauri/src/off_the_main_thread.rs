//! Which commands answer off the thread that asked (SC-2, #680).
//!
//! Tauri runs a synchronous command on the thread that handed it the request, which in the app
//! is the main thread: the one that pumps the window's events. A command that walks the plane's
//! files there holds the window for as long as the walk takes, and a plane with dozens of
//! workspaces and thousands of memories makes that a hitch a person sees. So the commands that
//! read every workspace, every memory or every session record are `async` and do their reading
//! on a blocking thread, as `workspace_repos` already did for git.
//!
//! **Every command that runs git answers off the window's thread too** (#1007): a git process
//! takes milliseconds to start before it does anything, so no such command can stay under the
//! 1 ms the window can spare. That is the worktree verbs (list, which piece a chat is in,
//! remove, done, merge, and New branch, which already was), and a workspace's create, its
//! at-risk reading and its remove. Create and remove also read the sidebar's model again under
//! its lock, which the watcher holds while it applies a change. Settings' two lists of grants
//! name who committed each of the project's grants, which asks git, so the dispatch grants'
//! read and revoke and the sandbox grants' read and revoke answer off it as well (#1543).
//!
//! `chat_usage` stays synchronous too: it reads one file of sixteen rows, about 30 µs, and walks
//! nothing. So does `workspace_focused`, which checks a name and hands the extensions' report
//! to a thread of its own.
//!
//! Smart close's offer, its start and its cancel stay synchronous, for a terminal's reason. They
//! run no git, and the offer and the cancel read only what the app holds in memory, well under
//! 1 ms. The start can also touch the disk: for a task that had reported and was about to be
//! ended, it stands that end down (`dispatched::stand_down`), which reads the project's dispatch
//! records to find the task's newest one and writes it again marked kept open (#1610). Only a
//! task's start does that, once. The start types its prompt into the chat's pane, and a cancel
//! pressed straight after must land after it: Tauri keeps that order only for synchronous
//! commands, and that order is worth the one read and write.
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
                crate::personas::persona_marks,
                crate::personas::persona_mark_set,
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
        // A persona's mark reads every persona's folder and writes its definition (#1454).
        check(
            "persona_mark_set",
            json!({ "plane": plane, "name": "scribe", "icon": "rocket", "colour": "teal" }),
        );
        let marks = check("persona_marks", json!({ "plane": plane }));
        assert!(
            marks
                .as_array()
                .is_some_and(|marks| marks.iter().any(|mark| mark["icon"] == "rocket")),
            "{marks}"
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
    fn every_dispatch_command_that_may_settle_against_git_answers_off_the_thread_that_asked() {
        // #1506, joined on train 64: a command that can end in this machine's acceptance of a
        // grant of the project's settles against git's history, which may take seconds. Each
        // refuses here (no dispatch 7 waits, and steward is no persona of this project), and
        // where the refusal is given is the point: inside the work, on a blocking thread.
        let dir = tempfile::tempdir().expect("a temp dir");
        let planes = Planes::telling(std::sync::Arc::new(|_| {}), crate::Shipped::default(), None);
        let plane = planes.open(dir.path());
        let app = mock_builder()
            .manage(planes)
            .invoke_handler(tauri::generate_handler![
                crate::dispatchgrants::dispatch_arrival,
                crate::dispatchgrants::answer_dispatch_arrival,
                crate::dispatchgrants::dispatch_gone_told,
                crate::dispatchgrants::allow_dispatch,
                crate::dispatchgrants::allow_dispatch_anywhere,
                crate::dispatchgrants::accept_project_dispatch,
                crate::dispatchgrants::allow_dispatch_to_any,
                crate::dispatchgrants::set_dispatch_workspace,
                crate::dispatchgrants::accept_project_dispatch_in,
                crate::dispatchgrants::give_back_dispatch,
                crate::dispatchgrants::add_dispatch_grant,
                crate::dispatchgrants::keep_dispatch_blocked,
            ])
            .build(tauri_context!(test = true))
            .expect("the app builds");
        tauri::WebviewWindowBuilder::new(&app, WINDOW, tauri::WebviewUrl::default())
            .build()
            .expect("the main window");
        let asking = std::thread::current().id();
        let allow = json!({
            "plane": plane, "id": 7, "level": "project", "also": [], "shown": "",
        });
        let asked = [
            ("dispatch_arrival", json!({ "plane": plane })),
            (
                "answer_dispatch_arrival",
                json!({ "plane": plane, "accepted": true, "shown": [], "listed": [] }),
            ),
            ("dispatch_gone_told", json!({ "plane": plane, "shown": [] })),
            ("allow_dispatch", allow.clone()),
            ("allow_dispatch_anywhere", allow),
            (
                "accept_project_dispatch",
                json!({ "plane": plane, "asking": "steward", "target": "devops" }),
            ),
            (
                "allow_dispatch_to_any",
                json!({ "plane": plane, "asking": "steward", "level": "project" }),
            ),
            (
                "set_dispatch_workspace",
                json!({
                    "plane": plane, "asking": "steward", "target": "devops",
                    "level": "project", "from": null, "to": "runners",
                }),
            ),
            (
                "accept_project_dispatch_in",
                json!({
                    "plane": plane, "asking": "steward", "target": "devops",
                    "workspace": "runners",
                }),
            ),
            (
                "give_back_dispatch",
                json!({ "plane": plane, "name": "devops" }),
            ),
            // #1465: a grant made in Settings, for everyone, settles too.
            (
                "add_dispatch_grant",
                json!({
                    "plane": plane, "asking": "steward", "target": "devops",
                    "level": "project", "workspace": null,
                }),
            ),
        ];
        // The list the grant store keeps of them is the one asked here.
        assert_eq!(
            asked
                .iter()
                .map(|(command, _)| *command)
                .collect::<Vec<_>>(),
            crate::dispatchgrants::SETTLES
        );

        let on_the_asking_thread: Vec<&str> = asked
            .into_iter()
            .filter(|(command, args)| ask(&app, command, args.clone()).answered_on == asking)
            .map(|(command, _)| command)
            .collect();
        assert!(
            on_the_asking_thread.is_empty(),
            "answered on the thread that asked, which in the app is the window's: \
             {on_the_asking_thread:?}"
        );
        // And the check can tell: a synchronous command of the same store answers where it
        // was asked.
        let kept = ask(
            &app,
            "keep_dispatch_blocked",
            json!({ "plane": plane, "id": 7 }),
        );
        assert_eq!(kept.answered_on, asking);
    }

    #[test]
    fn settings_reads_of_the_dispatch_grants_ask_git_off_the_thread_that_asked() {
        // #1543: Settings' list names who committed each of the project's grants, which asks
        // git once per pair. A revoke answers the same list. Here neither has a grant to
        // read, and the second refuses an id that names none: where each answers is the
        // point.
        let dir = tempfile::tempdir().expect("a temp dir");
        let planes = Planes::telling(std::sync::Arc::new(|_| {}), crate::Shipped::default(), None);
        let plane = planes.open(dir.path());
        let app = mock_builder()
            .manage(planes)
            .invoke_handler(tauri::generate_handler![
                crate::dispatchgrants::dispatch_grants,
                crate::dispatchgrants::revoke_dispatch_grant,
                crate::dispatchgrants::keep_dispatch_blocked,
            ])
            .build(tauri_context!(test = true))
            .expect("the app builds");
        tauri::WebviewWindowBuilder::new(&app, WINDOW, tauri::WebviewUrl::default())
            .build()
            .expect("the main window");
        let asking = std::thread::current().id();
        let asked = [
            ("dispatch_grants", json!({ "plane": plane })),
            (
                "revoke_dispatch_grant",
                json!({ "plane": plane, "id": "no such grant" }),
            ),
        ];
        assert_eq!(
            asked
                .iter()
                .map(|(command, _)| *command)
                .collect::<Vec<_>>(),
            crate::dispatchgrants::READS_HISTORY
        );
        let on_the_asking_thread: Vec<&str> = asked
            .into_iter()
            .filter(|(command, args)| ask(&app, command, args.clone()).answered_on == asking)
            .map(|(command, _)| command)
            .collect();
        assert!(
            on_the_asking_thread.is_empty(),
            "answered on the thread that asked, which in the app is the window's: \
             {on_the_asking_thread:?}"
        );
        let kept = ask(
            &app,
            "keep_dispatch_blocked",
            json!({ "plane": plane, "id": 7 }),
        );
        assert_eq!(kept.answered_on, asking);
    }

    #[test]
    fn settings_reads_of_the_sandbox_grants_ask_git_off_the_thread_that_asked() {
        // #1543: the Granted list names who committed each of the project's hosts, which asks
        // git, and a revoke answers the same list. Here there is no grant, and the revoke
        // refuses an id that names none: where each answers is the point.
        let dir = tempfile::tempdir().expect("a temp dir");
        let planes = Planes::telling(std::sync::Arc::new(|_| {}), crate::Shipped::default(), None);
        let plane = planes.open(dir.path());
        let app = mock_builder()
            .manage(planes)
            .invoke_handler(tauri::generate_handler![
                crate::sandboxing::sandbox_grants,
                crate::sandboxing::revoke_sandbox_grant,
            ])
            .build(tauri_context!(test = true))
            .expect("the app builds");
        tauri::WebviewWindowBuilder::new(&app, WINDOW, tauri::WebviewUrl::default())
            .build()
            .expect("the main window");
        let asking = std::thread::current().id();
        let asked = [
            ("sandbox_grants", json!({ "plane": plane })),
            (
                "revoke_sandbox_grant",
                json!({ "plane": plane, "id": "no such grant" }),
            ),
        ];
        assert_eq!(
            asked
                .iter()
                .map(|(command, _)| *command)
                .collect::<Vec<_>>(),
            crate::sandboxing::READS_HISTORY
        );
        let on_the_asking_thread: Vec<&str> = asked
            .into_iter()
            .filter(|(command, args)| ask(&app, command, args.clone()).answered_on == asking)
            .map(|(command, _)| command)
            .collect();
        assert!(
            on_the_asking_thread.is_empty(),
            "answered on the thread that asked, which in the app is the window's: \
             {on_the_asking_thread:?}"
        );
    }

    /// A project with no workspace in it, as the window holds it, with the commands that run
    /// git and the ones beside them that stay where they were asked.
    fn app_running_git(root: &Path) -> (tauri::App<MockRuntime>, PlaneId) {
        let planes = Planes::telling(std::sync::Arc::new(|_| {}), crate::Shipped::default(), None);
        let plane = planes.open(root);
        let app = mock_builder()
            .manage(planes)
            .invoke_handler(tauri::generate_handler![
                crate::worktrees::worktree_of_chat,
                crate::worktrees::worktree_list,
                crate::worktrees::worktree_remove,
                crate::worktrees::worktree_done,
                crate::worktrees::worktree_merge,
                crate::worktrees::worktree_add,
                crate::workspaces::workspace_at_risk,
                crate::smartclose::smart_close_offer,
                crate::smartclose::smart_close,
                crate::smartclose::cancel_smart_close,
            ])
            .build(tauri_context!(test = true))
            .expect("the app builds");
        tauri::WebviewWindowBuilder::new(&app, WINDOW, tauri::WebviewUrl::default())
            .build()
            .expect("the main window");
        (app, plane)
    }

    /// What the commands that run git are asked with, in a project with no workspace: each is
    /// refused, or finds nothing, inside its work.
    ///
    /// `workspace_create` and `workspace_remove` are not here: they take the app's handle to
    /// tell the extensions, and the mock runtime cannot hand them one. Each runs its work in
    /// `crate::off_the_window`, as `workspace_at_risk` does.
    fn the_git_commands(plane: &PlaneId, root: &Path) -> Vec<(&'static str, Value)> {
        let piece = json!({
            "plane": plane, "workspace": "alpha", "repo": "thing", "piece": "piece",
        });
        let mut remove = piece.clone();
        remove["force"] = json!(false);
        vec![
            (
                "worktree_of_chat",
                json!({ "plane": plane, "cwd": root.join("workspaces/alpha/thing") }),
            ),
            (
                "worktree_list",
                json!({ "plane": plane, "workspace": "alpha", "repo": "thing" }),
            ),
            ("worktree_remove", remove),
            ("worktree_done", piece.clone()),
            ("worktree_merge", piece),
            (
                "worktree_add",
                json!({ "plane": plane, "workspace": "alpha", "repo": "thing", "branch": null }),
            ),
            (
                "workspace_at_risk",
                json!({ "plane": plane, "workspace": "alpha" }),
            ),
        ]
    }

    #[test]
    fn the_commands_that_run_git_answer_off_the_thread_that_asked() {
        // #1007. Where each answers is the point, not what: a refusal given inside the work is
        // given on the blocking thread the work runs on.
        let dir = tempfile::tempdir().expect("a temp dir");
        let (app, plane) = app_running_git(dir.path());
        let asking = std::thread::current().id();

        let on_the_asking_thread: Vec<&str> = the_git_commands(&plane, dir.path())
            .into_iter()
            .filter(|(command, args)| ask(&app, command, args.clone()).answered_on == asking)
            .map(|(command, _)| command)
            .collect();

        assert!(
            on_the_asking_thread.is_empty(),
            "answered on the thread that asked, which in the app is the window's: \
             {on_the_asking_thread:?}"
        );
    }

    #[test]
    fn smart_close_and_its_cancel_answer_on_the_thread_that_asked_so_they_keep_their_order() {
        // #1007: the start types its prompt into the pane, and a cancel pressed after it must
        // land after it. Each refuses or does nothing here, as there is no chat 7.
        let dir = tempfile::tempdir().expect("a temp dir");
        let (app, plane) = app_running_git(dir.path());
        let asking = std::thread::current().id();
        let chat = json!({ "plane": plane, "session": 7 });

        for command in ["smart_close_offer", "smart_close", "cancel_smart_close"] {
            assert_eq!(
                ask(&app, command, chat.clone()).answered_on,
                asking,
                "{command}"
            );
        }
    }

    /// How long the commands #1007 lists hold the thread that asked, five asks each, printed.
    fn print_how_long_each_holds(app: &tauri::App<MockRuntime>, cases: &[(&str, Value)]) {
        for (command, args) in cases {
            let mut held: Vec<(Duration, Duration)> = (0..5)
                .map(|_| {
                    let started = Instant::now();
                    let asked = ask(app, command, args.clone());
                    (asked.held_for, started.elapsed())
                })
                .collect();
            held.sort();
            println!(
                "  {command}: asking thread held for a median of {:?} (min {:?}, max {:?}); \
                 answered after {:?}",
                held[2].0, held[0].0, held[4].0, held[2].1
            );
        }
    }

    /// How long the commands #1007 lists hold the thread that asked: `cargo test -p purlis-app
    /// off_the_main_thread -- --ignored --nocapture`. First in a project with nothing in it,
    /// which runs anywhere; then, where `git init` may write a `.git` folder, in one with a
    /// clone and a branch of it.
    #[test]
    #[ignore = "a measurement, printed; run it by hand"]
    fn how_long_the_commands_that_run_git_hold_the_thread_that_asked() {
        // What one git process costs before it does anything: the floor under every command
        // that runs one, and so the least each held the window for while it was synchronous.
        let mut spawns: Vec<Duration> = (0..5)
            .map(|_| {
                let started = Instant::now();
                purlis_core::forklock::output(std::process::Command::new("git").arg("--version"))
                    .expect("git runs");
                started.elapsed()
            })
            .collect();
        spawns.sort();
        println!("one `git --version`: a median of {:?}", spawns[2]);

        let empty = tempfile::tempdir().expect("a temp dir");
        let (app, plane) = app_running_git(empty.path());
        let chat = json!({ "plane": plane, "session": 7 });
        let mut cases = the_git_commands(&plane, empty.path());
        cases.extend([
            ("smart_close_offer", chat.clone()),
            ("smart_close", chat.clone()),
            ("cancel_smart_close", chat),
        ]);
        println!("in a project with nothing in it:");
        print_how_long_each_holds(&app, &cases);

        let dir = tempfile::tempdir().expect("a temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("its path");
        let clone = root.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).expect("the clone's folder");
        std::fs::write(root.join("workspaces/alpha/workspace.md"), "# alpha\n").expect("its md");
        let git = |args: &[&str]| {
            purlis_core::forklock::output(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(&clone)
                    .args(["-c", "user.email=t@e.invalid", "-c", "user.name=t"])
                    .args(["-c", "commit.gpgsign=false"])
                    .args(args),
            )
            .is_ok_and(|ran| ran.status.success())
        };
        std::fs::write(clone.join("README.md"), "one\n").expect("a file");
        if !(git(&["init", "-q", "-b", "main", "."])
            && git(&["add", "-A"])
            && git(&["commit", "-q", "-m", "one"]))
        {
            println!("git could not make a clone here, so there is no measure with one");
            return;
        }
        let added = purlis_core::worktree::add(&root, "alpha", "thing", "piece", None)
            .expect("a branch of it");
        let (app, plane) = app_running_git(&root);
        let piece = json!({
            "plane": plane, "workspace": "alpha", "repo": "thing", "piece": "piece",
        });
        println!("in a project with a clone and a branch of it:");
        print_how_long_each_holds(
            &app,
            &[
                (
                    "worktree_of_chat",
                    json!({ "plane": plane, "cwd": added.path }),
                ),
                (
                    "worktree_list",
                    json!({ "plane": plane, "workspace": "alpha", "repo": "thing" }),
                ),
                ("worktree_done", piece.clone()),
                ("worktree_merge", piece),
                (
                    "workspace_at_risk",
                    json!({ "plane": plane, "workspace": "alpha" }),
                ),
            ],
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
