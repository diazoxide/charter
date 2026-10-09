//! charter's ACP client (HP-2, ADR 0080) against the fake harness as a scripted ACP agent.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use purlis_core::acp::{self, Chat, Event, Launch, NotStarted, Stop, TurnFailed};
use purlis_core::harness::asks::{Admitted, Answerer, Asks, Raised, Refused};
use purlis_core::harness::model::{Began, Deadline, Plan, Said, Session, Step, Turn, Usage};

const PATIENCE: Duration = Duration::from_secs(20);

/// As the host does first thing: a chat refuses to start while the host has a terminal.
fn leave_the_terminal() {
    static LEFT: std::sync::Once = std::sync::Once::new();
    LEFT.call_once(|| {
        use purlis_core::noterminal::{Left, leave};
        if let Left::Relaunched(code) = leave().expect("the tests leave their terminal") {
            std::process::exit(code);
        }
    });
}

fn launch(dir: &Path, flags: &[&str]) -> Launch {
    leave_the_terminal();
    let mut argv = vec![
        env!("CARGO_BIN_EXE_fake-harness").to_owned(),
        "--acp".to_owned(),
    ];
    argv.extend(flags.iter().map(|flag| (*flag).to_owned()));
    Launch {
        chat: "chat-1".to_owned(),
        argv,
        cwd: dir.to_path_buf(),
        env: Vec::new(),
        charter_mcp: None,
        patience: PATIENCE,
    }
}

fn start(dir: &Path, flags: &[&str]) -> (Arc<Chat>, acp::Events) {
    let (chat, events) =
        Chat::start(launch(dir, flags), Arc::new(Asks::new())).expect("the agent starts");
    (Arc::new(chat), events)
}

/// Every event until one `until` accepts, which is the last of them.
fn events_until(events: &acp::Events, until: impl Fn(&Event) -> bool) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        match events.recv_timeout(PATIENCE) {
            Ok(event) => {
                let done = until(&event);
                seen.push(event);
                if done {
                    return seen;
                }
            }
            Err(err) => panic!("no more events ({err}) after {seen:#?}"),
        }
    }
}

fn text(event: &Event) -> Option<&str> {
    match event {
        Event::Text(text) => Some(text),
        _ => None,
    }
}

#[test]
fn a_prompt_is_answered_and_its_turn_ends() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    assert_eq!(turn(&chat, "hello").expect("the turn"), Stop::EndTurn);
    let seen = events_until(&events, |event| text(event).is_some());
    assert_eq!(seen.iter().find_map(text), Some("you said: hello"));
}

/// What the client sent the agent, one message a line, as the fake recorded it.
fn recorded(record: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(record)
        .expect("the record")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON"))
        .collect()
}

fn sent<'a>(record: &'a [serde_json::Value], method: &str) -> Option<&'a serde_json::Value> {
    record.iter().find(|message| message["method"] == method)
}

#[test]
fn the_handshake_offers_no_file_system_and_no_terminal_and_hands_the_session_charter_s_server() {
    let dir = tempfile::tempdir().expect("a worktree");
    let record: PathBuf = dir.path().join("record.jsonl");
    let mut launch = launch(
        dir.path(),
        &["--acp-record", record.to_str().expect("UTF-8")],
    );
    launch.charter_mcp = Some(PathBuf::from("/bin/charter"));
    launch.env = vec![("CHARTER_ROOT".into(), "/project".into())];
    let (chat, _events) = Chat::start(launch, Arc::new(Asks::new())).expect("the agent starts");
    drop(chat);

    let record = recorded(&record);
    let init = &sent(&record, "initialize").expect("initialize")["params"];
    assert_eq!(init["protocolVersion"], 1);
    let offered = &init["clientCapabilities"];
    assert_eq!(offered["fs"]["readTextFile"], false, "{offered}");
    assert_eq!(offered["fs"]["writeTextFile"], false, "{offered}");
    assert_eq!(offered["terminal"], false, "{offered}");
    assert!(
        offered
            .get("elicitation")
            .is_none_or(serde_json::Value::is_null),
        "{offered}"
    );

    let new = &sent(&record, "session/new").expect("session/new")["params"];
    assert_eq!(new["cwd"], dir.path().to_str().expect("UTF-8"));
    assert_eq!(
        new["mcpServers"],
        serde_json::json!([{
            "name": purlis_core::chattools::SERVER,
            "command": "/bin/charter",
            "args": ["mcp"],
            "env": [{"name": "PURLIS_ROOT", "value": "/project"}],
        }])
    );
    assert!(sent(&record, "authenticate").is_none(), "{record:#?}");
}

#[test]
fn what_the_agent_answered_at_initialize_is_what_the_run_can_do() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, _events) = start(dir.path(), &[]);
    let negotiated = chat.negotiated();
    assert_eq!(negotiated.protocol, 1);
    assert_eq!(negotiated.agent.as_deref(), Some("fake-harness 0"));
    assert!(negotiated.resumes_by_id, "loadSession is resumes_by_id");
    assert!(negotiated.takes_images);
    // Silence is no.
    assert!(!negotiated.takes_audio);
    assert!(!negotiated.takes_embedded_context);
    assert_eq!(chat.session(), "fake-1");
}

#[test]
fn an_agent_on_another_protocol_version_does_not_start() {
    let dir = tempfile::tempdir().expect("a worktree");
    let refused = Chat::start(
        launch(dir.path(), &["--acp-version", "2"]),
        Arc::new(Asks::new()),
    )
    .expect_err("refused");
    assert_eq!(refused, NotStarted::Version { offered: 2 });
}

#[test]
fn an_agent_that_needs_a_login_does_not_start_and_charter_never_authenticates() {
    let dir = tempfile::tempdir().expect("a worktree");
    let record = dir.path().join("record.jsonl");
    let refused = Chat::start(
        launch(
            dir.path(),
            &[
                "--acp-login",
                "--acp-record",
                record.to_str().expect("UTF-8"),
            ],
        ),
        Arc::new(Asks::new()),
    )
    .expect_err("refused");
    assert_eq!(refused, NotStarted::Login);
    assert!(refused.to_string().contains("shell tab"), "{refused}");
    assert!(sent(&recorded(&record), "authenticate").is_none());
}

#[test]
fn a_program_that_is_not_there_does_not_start() {
    let dir = tempfile::tempdir().expect("a worktree");
    let mut launch = launch(dir.path(), &[]);
    launch.argv = vec![dir.path().join("no-such-agent").display().to_string()];
    let refused = Chat::start(launch, Arc::new(Asks::new())).expect_err("refused");
    assert!(matches!(refused, NotStarted::Spawn { .. }), "{refused:?}");
}

#[test]
fn a_turn_reports_its_plan_usage_and_tool_calls_in_the_neutral_model_between_its_start_and_end() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    turn(&chat, "hello").expect("the turn");
    let seen = events_until(&events, |event| {
        *event == Event::Said(Said::Turn(Turn::Ended))
    });
    assert_eq!(
        seen,
        vec![
            Event::Said(Said::Session(Session::Began(Began::Fresh))),
            Event::Said(Said::Turn(Turn::Began)),
            Event::Said(Said::Plan(Plan {
                steps: vec![
                    Step {
                        text: "Read the text".to_owned(),
                        done: true
                    },
                    Step {
                        text: "Say it back".to_owned(),
                        done: false
                    },
                ],
            })),
            Event::Said(Said::Usage(Usage {
                input_tokens: None,
                output_tokens: None,
                context_percent: Some(25),
            })),
            Event::ToolCall {
                id: "t1".to_owned(),
                title: "Read a file".to_owned(),
                kind: "read".to_owned(),
                status: "pending".to_owned(),
            },
            Event::ToolCallStatus {
                id: "t1".to_owned(),
                status: "completed".to_owned(),
            },
            Event::Text("you said: hello".to_owned()),
            Event::Said(Said::Turn(Turn::Ended)),
        ]
    );
}

/// A prompt too long to write is refused before it reaches the agent, and the chat goes on
/// (#1117): measured as written, so a prompt of quotes, each escaped, is refused at half the
/// length; a prompt just inside the bound is sent and answered whole.
#[test]
fn a_prompt_too_long_to_send_is_refused_and_the_chat_goes_on() {
    let dir = tempfile::tempdir().expect("a worktree");
    let record: PathBuf = dir.path().join("record.jsonl");
    let (chat, events) = start(
        dir.path(),
        &["--acp-record", record.to_str().expect("UTF-8")],
    );
    let most = acp::MOST_UNWRITTEN_BYTES;
    assert_eq!(turn(&chat, &"p".repeat(most)), Err(TurnFailed::TooLong));
    assert_eq!(
        turn(&chat, &"\"".repeat(most / 2)),
        Err(TurnFailed::TooLong)
    );

    assert_eq!(turn(&chat, "hello"), Ok(Stop::EndTurn));
    let seen = events_until(&events, |event| {
        *event == Event::Said(Said::Turn(Turn::Ended))
    });
    let began = Event::Said(Said::Turn(Turn::Began));
    assert_eq!(
        seen.iter().filter(|event| **event == began).count(),
        1,
        "{seen:#?}"
    );
    assert_eq!(seen.iter().find_map(text), Some("you said: hello"));

    let near = "p".repeat(most - 4096);
    assert_eq!(turn(&chat, &near), Ok(Stop::EndTurn));
    let lines = std::fs::read_to_string(&record).expect("the record");
    let prompts: Vec<&str> = lines
        .lines()
        .filter(|line| line.contains("\"session/prompt\""))
        .collect();
    assert_eq!(
        prompts.len(),
        2,
        "only the prompts that fit reached the agent"
    );
    assert!(prompts[0].contains("\"hello\""));
    assert!(prompts[1].len() < most, "{}", prompts[1].len());
}

fn the_operator() -> Answerer {
    Answerer::admitted(Admitted::LocalUi).expect("the window answers")
}

/// Prompts `text` on a thread of its own, since a turn with an ask waits on its answer. The
/// receiver hears how the turn ended.
fn prompt_aside(chat: &Arc<Chat>, text: &str) -> Receiver<Result<Stop, TurnFailed>> {
    let chat = Arc::clone(chat);
    let text = text.to_owned();
    let (ended, turn) = std::sync::mpsc::channel();
    std::thread::spawn(move || ended.send(chat.prompt(&text)));
    turn
}

/// One turn, waited for at most [`PATIENCE`]: a turn that never ends fails the test instead of
/// hanging it.
fn turn(chat: &Arc<Chat>, text: &str) -> Result<Stop, TurnFailed> {
    ended(&prompt_aside(chat, text))
}

fn ended(turn: &Receiver<Result<Stop, TurnFailed>>) -> Result<Stop, TurnFailed> {
    turn.recv_timeout(PATIENCE).expect("the turn ended")
}

fn raised(events: &acp::Events) -> Raised {
    let seen = events_until(events, |event| matches!(event, Event::Raised(_)));
    match seen.into_iter().last() {
        Some(Event::Raised(raised)) => raised,
        other => panic!("not an ask: {other:?}"),
    }
}

#[test]
fn a_permission_request_waits_with_no_deadline_until_a_human_answers_and_the_first_answer_wins() {
    let dir = tempfile::tempdir().expect("a worktree");
    let asks = Arc::new(Asks::new());
    let (chat, events) =
        Chat::start(launch(dir.path(), &[]), Arc::clone(&asks)).expect("the agent starts");
    let chat = Arc::new(chat);
    let turn = prompt_aside(&chat, "ask");

    let ask = raised(&events);
    assert_eq!(ask.chat, "chat-1");
    assert_eq!(ask.ask.deadline, Deadline::None);
    assert_eq!(
        ask.ask
            .options
            .iter()
            .map(|option| option.id.as_str())
            .collect::<Vec<_>>(),
        ["once", "no"]
    );
    assert_eq!(asks.pending(std::time::Instant::now()), vec![ask.clone()]);

    // Nothing answers it on a timer: the turn is still waiting.
    std::thread::sleep(Duration::from_millis(300));
    assert!(turn.try_recv().is_err(), "a timeout never answers");
    assert_eq!(
        asks.expire(std::time::Instant::now() + Duration::from_secs(86_400)),
        vec![]
    );

    let applied = chat
        .answer(&ask.id, "once", the_operator())
        .expect("answered");
    assert_eq!(applied.choice.id, "once");
    assert_eq!(
        chat.answer(&ask.id, "no", the_operator()),
        Err(Refused::AnsweredElsewhere)
    );
    assert_eq!(ended(&turn), Ok(Stop::EndTurn));
    let seen = events_until(&events, |event| text(event).is_some());
    assert_eq!(seen.iter().find_map(text), Some("chose once"));
}

#[test]
fn cancelling_a_turn_answers_its_waiting_request_cancelled_and_withdraws_the_ask() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    let turn = prompt_aside(&chat, "ask");
    let ask = raised(&events);

    chat.cancel();
    assert_eq!(ended(&turn), Ok(Stop::Cancelled));
    let seen = events_until(&events, |event| text(event).is_some());
    assert_eq!(seen.iter().find_map(text), Some("cancelled"));
    assert_eq!(
        chat.answer(&ask.id, "once", the_operator()),
        Err(Refused::Withdrawn)
    );
}

#[test]
fn cancelling_a_working_turn_ends_it_cancelled() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, _events) = start(dir.path(), &[]);
    let turn = prompt_aside(&chat, "wait");
    // The prompt has to reach the agent before the cancel can mean anything to it.
    std::thread::sleep(Duration::from_millis(200));
    chat.cancel();
    assert_eq!(ended(&turn), Ok(Stop::Cancelled));
}

#[test]
fn a_client_method_charter_does_not_offer_is_refused_and_reported() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    assert_eq!(turn(&chat, "fs"), Ok(Stop::EndTurn));
    let seen = events_until(&events, |event| text(event).is_some());
    assert!(
        seen.contains(&Event::Refused {
            method: "fs/read_text_file".to_owned()
        }),
        "{seen:#?}"
    );
    // JSON-RPC's "method not found".
    assert_eq!(seen.iter().find_map(text), Some("refused -32601"));
}

#[test]
fn a_chat_dropped_ends_its_agent() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    drop(chat);
    events_until(&events, |event| *event == Event::Ended);
}

/// A project with one approved profile, `work`, of kind opencode, running `program`: what a
/// chat started from the window starts from ([`purlis_core::start::ready`]).
fn an_opencode_project(program: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a project");
    std::fs::write(dir.path().join("charter.toml"), "").expect("charter.toml");
    std::fs::write(
        dir.path().join(purlis_core::profiles::LOCAL_FILE),
        format!("[harness.work]\nkind = \"opencode\"\ncommand = [{program:?}]\n"),
    )
    .expect("the profile");
    let set = purlis_core::profiles::current(dir.path());
    purlis_core::profiletrust::record_launched(
        dir.path(),
        "work",
        &purlis_core::profiletrust::fingerprint(set.get("work").expect("declared")),
    )
    .expect("approved");
    dir
}

/// A chat on `project`'s `work` profile, started at level 3 the way the host starts one: the
/// start every chat makes, then its ACP agent from it, in the chat's environment from `app`.
fn started_at_level_3(
    project: &Path,
    app: Vec<(std::ffi::OsString, std::ffi::OsString)>,
) -> (Arc<Chat>, acp::Events) {
    leave_the_terminal();
    let start = purlis_core::start::Start {
        profile: Some("work".to_owned()),
        name: "level three".to_owned(),
        cwd: Some(project.to_path_buf()),
        ..purlis_core::start::Start::default()
    };
    let ready = purlis_core::start::ready(&start, project).expect("the chat may start");
    let launch = Launch::for_start(
        &ready,
        acp::Host {
            chat: "7".to_owned(),
            app_env: app,
            operator: &[],
            strip: &[],
            own: vec![("CHARTER_SESSION_ID".to_owned(), "7".to_owned())],
            git_hooks: None,
            charter: None,
        },
    )
    .expect("offered at level 3");
    let (chat, events) = Chat::start(
        Launch {
            patience: PATIENCE.max(launch.patience),
            ..launch
        },
        Arc::new(Asks::new()),
    )
    .expect("it starts over ACP");
    (Arc::new(chat), events)
}

/// The board's view of a chat, moved by every event it hears, until `until` accepts one.
fn board_until(
    board: &mut purlis_core::state::Chat,
    events: &acp::Events,
    until: impl Fn(&Event) -> bool,
) -> Vec<Event> {
    let seen = events_until(events, until);
    for said in seen.iter().filter_map(Event::said) {
        board.heard(&said);
    }
    seen
}

#[test]
fn an_opencode_chat_started_at_level_3_runs_over_acp_and_its_board_follows_its_turns() {
    use purlis_core::state::State;
    let project = an_opencode_project(env!("CARGO_BIN_EXE_fake-harness"));
    let (chat, events) = started_at_level_3(project.path(), Vec::new());
    let mut board = purlis_core::state::Chat::new();
    board_until(&mut board, &events, |event| {
        matches!(event, Event::Said(Said::Session(_)))
    });
    assert_eq!(board.state(), State::Waiting);

    let turn = prompt_aside(&chat, "ask");
    let seen = board_until(&mut board, &events, |event| {
        matches!(event, Event::Raised(_))
    });
    assert!(board.asking() && board.needs_you(), "{seen:#?}");
    let Some(Event::Raised(ask)) = seen.last() else {
        panic!("{seen:#?}");
    };
    chat.answer(&ask.id, "once", the_operator())
        .expect("the operator answers");
    assert_eq!(ended(&turn), Ok(Stop::EndTurn));
    board_until(&mut board, &events, |event| {
        *event == Event::Said(Said::Turn(Turn::Ended))
    });
    assert_eq!(board.state(), State::Waiting);
    assert!(board.needs_you() && !board.asking());
    // The agent runs in the chat's environment: what the host set for the chat is there.
    assert_eq!(
        turn_text(&chat, &events, "env CHARTER_SESSION_ID"),
        "CHARTER_SESSION_ID=7"
    );
}

/// One turn's first piece of text.
fn turn_text(chat: &Arc<Chat>, events: &acp::Events, prompt: &str) -> String {
    assert_eq!(turn(chat, prompt), Ok(Stop::EndTurn));
    let seen = events_until(events, |event| text(event).is_some());
    seen.iter().find_map(text).unwrap_or_default().to_owned()
}

/// HP-2's acceptance on a real opencode: a chat on an opencode profile, started at level 3 the
/// way the host starts one, runs a turn over ACP with `fs` and `terminal` both off (ADR 0080
/// §2), and its board follows the turn. It needs opencode on `PATH` with a model it can reach
/// (opencode's own free models answer with no login), and it spends one short turn of it, so it
/// runs only when asked, on a HOME of its own: `env -i HOME=<scratch> PATH=<with opencode>
/// cargo test -p fake-harness --test over_acp -- --ignored`.
#[test]
#[ignore = "runs the real opencode and spends a turn of its model"]
fn an_opencode_chat_runs_over_acp() {
    use purlis_core::state::State;
    let project = an_opencode_project("opencode");
    let (chat, events) = started_at_level_3(project.path(), std::env::vars_os().collect());
    assert_eq!(chat.negotiated().protocol, 1);
    assert!(chat.negotiated().resumes_by_id, "{:?}", chat.negotiated());
    let mut board = purlis_core::state::Chat::new();

    assert_eq!(
        chat.prompt("Reply with the single word pong, and use no tools."),
        Ok(Stop::EndTurn)
    );
    let seen = board_until(&mut board, &events, |event| {
        *event == Event::Said(Said::Turn(Turn::Ended))
    });
    let reply: String = seen.iter().filter_map(text).collect();
    assert!(reply.to_lowercase().contains("pong"), "{seen:#?}");
    assert!(
        seen.contains(&Event::Said(Said::Turn(Turn::Began))),
        "{seen:#?}"
    );
    assert_eq!(board.state(), State::Waiting);
    assert!(board.needs_you());
}

#[test]
fn a_chat_answers_only_its_own_asks() {
    let dir = tempfile::tempdir().expect("a worktree");
    let asks = Arc::new(Asks::new());
    let (mine, _mine_events) =
        Chat::start(launch(dir.path(), &[]), Arc::clone(&asks)).expect("the agent starts");
    let (theirs, their_events) = Chat::start(
        Launch {
            chat: "chat-2".to_owned(),
            ..launch(dir.path(), &[])
        },
        Arc::clone(&asks),
    )
    .expect("the agent starts");
    let theirs = Arc::new(theirs);
    let turn = prompt_aside(&theirs, "ask");
    let ask = raised(&their_events);

    assert_eq!(
        mine.answer(&ask.id, "once", the_operator()),
        Err(Refused::Unknown)
    );
    assert_eq!(asks.pending(std::time::Instant::now()), vec![ask.clone()]);
    theirs
        .answer(&ask.id, "no", the_operator())
        .expect("its own chat answers it");
    assert_eq!(ended(&turn), Ok(Stop::EndTurn));
}

#[test]
fn a_line_longer_than_the_cap_ends_the_chat() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    let too_long = acp::MOST_LINE_BYTES + 1;
    assert_eq!(
        turn(&chat, &format!("long {too_long}")),
        Err(TurnFailed::Gone)
    );
    events_until(&events, |event| *event == Event::Ended);
}

#[test]
fn notifications_charter_has_no_handler_for_are_dropped_and_not_kept() {
    // The SDK keeps an unhandled notification that names a session, for a handler that may
    // come later; charter adds none, so a flood of them would be kept for the chat's life.
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, _events) = start(dir.path(), &[]);
    assert_eq!(turn(&chat, "flood 500"), Ok(Stop::EndTurn));
    assert_eq!(chat.notifications_ignored(), 500);
}

#[test]
fn an_agent_that_never_answers_the_handshake_is_ended_in_time() {
    let dir = tempfile::tempdir().expect("a worktree");
    let started = std::time::Instant::now();
    let refused = Chat::start(
        Launch {
            patience: Duration::from_millis(300),
            ..launch(dir.path(), &["--acp-silent"])
        },
        Arc::new(Asks::new()),
    )
    .expect_err("refused");
    assert!(matches!(refused, NotStarted::Handshake(_)), "{refused:?}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn an_answer_racing_a_cancel_never_reaches_the_agent_after_the_cancel() {
    // ACP: once `session/cancel` is sent, every request the client still owes is answered
    // `cancelled`. Whichever wins, the operator is told what the agent was given.
    for _ in 0..30 {
        let dir = tempfile::tempdir().expect("a worktree");
        let (chat, events) = start(dir.path(), &[]);
        let turn = prompt_aside(&chat, "ask");
        let ask = raised(&events);
        let both = Arc::new(std::sync::Barrier::new(2));
        let answering = {
            let (chat, both, id) = (Arc::clone(&chat), Arc::clone(&both), ask.id.clone());
            std::thread::spawn(move || {
                both.wait();
                chat.answer(&id, "once", the_operator())
            })
        };
        both.wait();
        chat.cancel();
        let answered = answering.join().expect("the answering thread");
        ended(&turn).expect("the turn ended");
        let seen = events_until(&events, |event| text(event).is_some());
        let said = seen.iter().find_map(text).expect("the agent said");
        match answered {
            Ok(_) => assert_eq!(said, "chose once"),
            Err(why) => {
                assert_eq!(why, Refused::Withdrawn);
                assert_eq!(said, "cancelled");
            }
        }
    }
}

#[test]
fn a_request_or_an_update_for_another_session_is_refused() {
    let dir = tempfile::tempdir().expect("a worktree");
    let asks = Arc::new(Asks::new());
    let (chat, events) =
        Chat::start(launch(dir.path(), &[]), Arc::clone(&asks)).expect("the agent starts");
    let chat = Arc::new(chat);
    assert_eq!(turn(&chat, "foreign"), Ok(Stop::EndTurn));
    let seen = events_until(&events, |event| {
        *event == Event::Said(Said::Turn(Turn::Ended))
    });
    assert!(
        !seen.iter().any(|event| matches!(event, Event::Raised(_))),
        "{seen:#?}"
    );
    let said: Vec<&str> = seen.iter().filter_map(text).collect();
    // JSON-RPC's "invalid params".
    assert_eq!(said, ["foreign answered -32602"]);
    assert!(asks.pending(std::time::Instant::now()).is_empty());
}

#[test]
fn a_request_the_agent_withdraws_withdraws_its_ask() {
    let dir = tempfile::tempdir().expect("a worktree");
    let asks = Arc::new(Asks::new());
    let (chat, events) =
        Chat::start(launch(dir.path(), &[]), Arc::clone(&asks)).expect("the agent starts");
    let chat = Arc::new(chat);
    assert_eq!(turn(&chat, "withdraw"), Ok(Stop::EndTurn));
    let seen = events_until(&events, |event| {
        *event == Event::Said(Said::Turn(Turn::Ended))
    });
    let ask = seen
        .iter()
        .find_map(|event| match event {
            Event::Raised(raised) => Some(raised.clone()),
            _ => None,
        })
        .expect("the ask was raised");
    assert!(
        seen.iter()
            .filter_map(text)
            .any(|said| said.starts_with("withdrew")),
        "{seen:#?}"
    );
    assert!(asks.pending(std::time::Instant::now()).is_empty());
    assert_eq!(
        chat.answer(&ask.id, "once", the_operator()),
        Err(Refused::Withdrawn)
    );
}

#[test]
fn an_agent_that_exits_mid_turn_is_gone() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, _events) = start(dir.path(), &[]);
    assert_eq!(turn(&chat, "die"), Err(TurnFailed::Gone));
}

#[test]
fn events_nobody_reads_are_dropped_past_the_bound_and_the_chat_goes_on() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    let many = acp::EVENTS_HELD + 1000;
    assert_eq!(turn(&chat, &format!("chunks {many}")), Ok(Stop::EndTurn));
    assert!(chat.events_dropped() > 0);
    assert!(events.try_iter().count() <= acp::EVENTS_HELD);
    assert_eq!(turn(&chat, "hello"), Ok(Stop::EndTurn));
}

#[test]
fn an_agent_whose_descendant_holds_its_stdout_is_still_ended_in_time() {
    // Ending the agent's group cannot end a process that left it, and that process can keep
    // the pipe open: the handshake's timeout must not wait for its end.
    let patience = Duration::from_secs(2);
    // A freshly built fake, or one on a loaded machine, can take longer than the patience to
    // start at all, and is then ended before it leaves anything behind; that try says nothing,
    // so it is made again.
    'attempt: for _ in 0..20 {
        let dir = tempfile::tempdir().expect("a worktree");
        let record = dir.path().join("record.jsonl");
        let launch = Launch {
            patience,
            ..launch(
                dir.path(),
                &[
                    "--acp-silent",
                    "--acp-escape",
                    "--acp-record",
                    record.to_str().expect("UTF-8"),
                ],
            )
        };
        let (done, outcome) = std::sync::mpsc::channel();
        // On a thread of its own, so that a start that hangs fails this test, not hangs it.
        std::thread::spawn(move || done.send(Chat::start(launch, Arc::new(Asks::new())).err()));
        let escaped = |record: &Path| {
            std::fs::read_to_string(record)
                .ok()
                .and_then(|text| text.lines().next().map(str::to_owned))
                .and_then(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
                .and_then(|line| line["escaped"].as_u64())
        };
        // The bound runs from when the escaping process is there, not from the call.
        let deadline = std::time::Instant::now() + PATIENCE;
        let (escaped, outcome) = loop {
            if let Some(pid) = escaped(&record) {
                break (pid, outcome.recv_timeout(patience + Duration::from_secs(3)));
            }
            if let Ok(early) = outcome.try_recv() {
                // Ended before the fake got as far as leaving anything: try again.
                match escaped(&record) {
                    Some(pid) => break (pid, Ok(early)),
                    None => {
                        assert!(early.is_some(), "a silent agent started");
                        continue 'attempt;
                    }
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the fake never started"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let _ = std::process::Command::new("kill")
            .arg(escaped.to_string())
            .status();
        let refused = outcome.expect("start returned in time").expect("refused");
        assert!(matches!(refused, NotStarted::Handshake(_)), "{refused:?}");
        return;
    }
    panic!("the fake never started in time to leave a process behind");
}

#[test]
fn unread_text_is_bounded_in_bytes_and_a_long_reply_comes_in_pieces() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    // 20 pieces of 2 MiB: more than twice what may wait unread.
    assert_eq!(turn(&chat, "big 20 2097152"), Ok(Stop::EndTurn));
    assert!(chat.events_dropped() > 0);
    let held: Vec<Event> = events.try_iter().collect();
    let texts: Vec<usize> = held.iter().filter_map(text).map(str::len).collect();
    assert!(
        texts.iter().all(|len| *len <= acp::MOST_TEXT_BYTES),
        "{texts:?}"
    );
    assert!(
        texts.iter().sum::<usize>() <= acp::MOST_BYTES_HELD,
        "{}",
        texts.iter().sum::<usize>()
    );
    // What was read frees its room.
    assert_eq!(turn(&chat, "hello"), Ok(Stop::EndTurn));
    let seen = events_until(&events, |event| {
        *event == Event::Said(Said::Turn(Turn::Ended))
    });
    assert!(
        seen.contains(&Event::Text("you said: hello".to_owned())),
        "{seen:#?}"
    );
}

#[test]
fn the_agent_does_not_inherit_the_host_s_relaunch_marker() {
    // An app started from a chat would otherwise think it had already left its terminal.
    let dir = tempfile::tempdir().expect("a worktree");
    let marker = purlis_core::noterminal::RELAUNCHED_ENV;
    let (chat, events) = Chat::start(
        Launch {
            env: vec![(marker.into(), "1".into())],
            ..launch(dir.path(), &[])
        },
        Arc::new(Asks::new()),
    )
    .expect("the agent starts");
    let chat = Arc::new(chat);
    assert_eq!(turn(&chat, &format!("env {marker}")), Ok(Stop::EndTurn));
    let seen = events_until(&events, |event| text(event).is_some());
    assert_eq!(
        seen.iter().find_map(text),
        Some(format!("{marker}=unset").as_str())
    );
}

#[test]
fn unread_plans_are_bounded_in_bytes_and_each_plan_in_steps_and_step_size() {
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    // 100 plans of 100 steps of 8 KiB: each kept plan still weighs 256 KiB, so 100 of them
    // are past what may wait unread.
    assert_eq!(turn(&chat, "plans 100 8192"), Ok(Stop::EndTurn));
    assert!(chat.events_dropped() > 0);
    let mut held = 0;
    for event in events.try_iter() {
        if let Event::Said(Said::Plan(plan)) = event {
            assert!(
                plan.steps.len() <= acp::MOST_PLAN_STEPS,
                "{}",
                plan.steps.len()
            );
            for step in &plan.steps {
                assert!(
                    step.text.len() <= acp::MOST_STEP_BYTES,
                    "{}",
                    step.text.len()
                );
                held += step.text.len();
            }
        }
    }
    assert!(held > 0);
    assert!(held <= acp::MOST_BYTES_HELD, "{held}");
}

#[test]
fn an_ask_carrying_more_than_the_limit_is_answered_cancelled_and_never_raised() {
    // Not cut short: a command shown for approval without its tail is not the command that runs.
    let dir = tempfile::tempdir().expect("a worktree");
    let asks = Arc::new(Asks::new());
    let (chat, events) =
        Chat::start(launch(dir.path(), &[]), Arc::clone(&asks)).expect("the agent starts");
    let chat = Arc::new(chat);
    let past = acp::MOST_ASK_BYTES + 1;
    assert_eq!(turn(&chat, &format!("bigask {past}")), Ok(Stop::EndTurn));
    let seen = events_until(&events, |event| {
        *event == Event::Said(Said::Turn(Turn::Ended))
    });
    assert!(
        !seen.iter().any(|event| matches!(event, Event::Raised(_))),
        "{seen:#?}"
    );
    assert_eq!(seen.iter().find_map(text), Some("answered cancelled"));
    assert_eq!(chat.asks_refused(), 1);
    assert!(asks.pending(std::time::Instant::now()).is_empty());
    // One just under the limit is raised as any other.
    let turn = prompt_aside(&chat, "bigask 1000");
    raised(&events);
    chat.cancel();
    assert_eq!(ended(&turn), Ok(Stop::EndTurn));
}

#[test]
fn asks_past_the_open_limit_are_answered_cancelled_without_being_raised() {
    let dir = tempfile::tempdir().expect("a worktree");
    let asks = Arc::new(Asks::new());
    let (chat, events) =
        Chat::start(launch(dir.path(), &[]), Arc::clone(&asks)).expect("the agent starts");
    let chat = Arc::new(chat);
    let past = 40;
    let asked = acp::MOST_OPEN_ASKS + past;
    let turn = prompt_aside(&chat, &format!("asks {asked}"));
    let mut raised = 0;
    let mut early = 0;
    while raised < acp::MOST_OPEN_ASKS || early < past {
        match events.recv_timeout(PATIENCE).expect("an event") {
            Event::Raised(_) => raised += 1,
            Event::Text(text) => {
                assert_eq!(text, "early cancelled");
                early += 1;
            }
            _ => {}
        }
    }
    assert_eq!(raised, acp::MOST_OPEN_ASKS);
    assert_eq!(early, past);
    assert_eq!(
        asks.pending(std::time::Instant::now()).len(),
        acp::MOST_OPEN_ASKS
    );
    chat.cancel();
    assert_eq!(ended(&turn), Ok(Stop::Cancelled));
    assert!(asks.pending(std::time::Instant::now()).is_empty());
}

#[test]
fn a_host_that_fell_behind_still_hears_the_chat_end_and_then_that_nothing_more_comes() {
    // The end is never one of the events dropped past the bound: a host reading the backlog
    // late reads it to the end, and then reads that the channel is closed, not a wait forever.
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = start(dir.path(), &[]);
    let many = acp::EVENTS_HELD + 1000;
    assert_eq!(turn(&chat, &format!("chunks {many}")), Ok(Stop::EndTurn));
    assert!(chat.events_dropped() > 0, "the unread events are full");
    assert_eq!(turn(&chat, "die"), Err(TurnFailed::Gone));
    let mut read = 0;
    loop {
        match events.recv_timeout(PATIENCE) {
            Ok(Event::Ended) => break,
            Ok(_) => read += 1,
            Err(err) => panic!("no end after {read} events: {err}"),
        }
    }
    assert!(read <= acp::EVENTS_HELD, "{read}");
    assert_eq!(
        events.recv_timeout(Duration::from_secs(5)),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
    );
}

#[test]
fn the_kill_switch_ends_a_chat_still_held_and_withdraws_its_asks() {
    // ADR 0080 §5.5, ADR 0071: the kill switch sends nothing and waits for nothing. A chat a
    // thread holds while it waits on a turn is never dropped, so the kill has to reach the
    // agent's group through the chat itself, and the ask it left open goes with it.
    let dir = tempfile::tempdir().expect("a worktree");
    let asks = Arc::new(Asks::new());
    let (chat, events) =
        Chat::start(launch(dir.path(), &[]), Arc::clone(&asks)).expect("the agent starts");
    let chat = Arc::new(chat);
    let turn = prompt_aside(&chat, "ask");
    raised(&events);

    chat.kill();

    assert_eq!(ended(&turn), Err(TurnFailed::Gone));
    events_until(&events, |event| *event == Event::Ended);
    assert_eq!(asks.pending(std::time::Instant::now()), Vec::new());
}

#[test]
fn the_agent_gets_the_chat_s_environment_and_nothing_else_of_the_host_s() {
    // ADR 0080 §1: the agent is started like a terminal chat's program, which starts from an
    // empty environment plus what charter keeps for it (`purlis_core::chatenv`).
    assert!(std::env::var_os("HOME").is_some(), "the host has a HOME");
    let dir = tempfile::tempdir().expect("a worktree");
    let (chat, events) = Chat::start(
        Launch {
            env: vec![("CHARTER_CHAT_ONLY".into(), "kept".into())],
            ..launch(dir.path(), &[])
        },
        Arc::new(Asks::new()),
    )
    .expect("the agent starts");
    let chat = Arc::new(chat);
    let mut said = Vec::new();
    for name in ["CHARTER_CHAT_ONLY", "HOME"] {
        assert_eq!(turn(&chat, &format!("env {name}")), Ok(Stop::EndTurn));
        let seen = events_until(&events, |event| text(event).is_some());
        said.extend(seen.iter().find_map(text).map(str::to_owned));
    }
    assert_eq!(said, ["CHARTER_CHAT_ONLY=kept", "HOME=unset"]);
}

#[test]
fn the_kill_switch_ends_a_chat_whose_escaped_descendant_holds_its_stdout() {
    // A process that left the agent's group keeps its stdout open, so the connection never sees
    // the agent go. The kill ends the chat by charter's own act all the same: the turn is gone,
    // its asks are withdrawn and cannot be answered, and the host hears the end.
    let dir = tempfile::tempdir().expect("a worktree");
    let record = dir.path().join("record.jsonl");
    let asks = Arc::new(Asks::new());
    let (chat, events) = Chat::start(
        launch(
            dir.path(),
            &[
                "--acp-escape",
                "--acp-record",
                record.to_str().expect("UTF-8"),
            ],
        ),
        Arc::clone(&asks),
    )
    .expect("the agent starts");
    let escaped = std::fs::read_to_string(&record)
        .ok()
        .and_then(|text| text.lines().next().map(str::to_owned))
        .and_then(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
        .and_then(|line| line["escaped"].as_u64())
        .expect("a process left the agent's group");
    let chat = Arc::new(chat);
    let turn = prompt_aside(&chat, "ask");
    let ask = raised(&events);

    chat.kill();

    let gone = ended(&turn);
    let seen = events_until(&events, |event| *event == Event::Ended);
    let answered = chat.answer(&ask.id, "once", the_operator());
    let _ = std::process::Command::new("kill")
        .arg(escaped.to_string())
        .status();
    assert_eq!(gone, Err(TurnFailed::Gone), "{seen:#?}");
    assert_eq!(asks.pending(std::time::Instant::now()), Vec::new());
    assert!(answered.is_err(), "{answered:?}");
}
