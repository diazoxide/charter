//! The fake harness, run the way the app runs a real one: inside a charter session.
#![cfg(unix)]

use std::path::Path;
use std::time::{Duration, Instant};

use charter_core::engine::{AlacrittyEngine, Screen, Size};
use charter_core::session::{Exit, Session, Spec};

const SIZE: Size = Size {
    columns: 100,
    rows: 30,
};
const PATIENCE: Duration = Duration::from_secs(20);

fn harness(args: &[&str]) -> Session {
    Session::spawn(
        Spec::new(env!("CARGO_BIN_EXE_fake-harness"), SIZE).args(args),
        Box::new(AlacrittyEngine::new(SIZE, 1000)),
    )
    .expect("the fake harness starts")
}

/// The screen once `enough` is true of it, or a failure saying what it showed instead.
///
/// Nothing may read the screen once and expect the program's whole output to be on it —
/// not even after the program has exited. `Session::wait` answers when the PROGRAM is
/// gone, and the bytes it wrote are still travelling: the reader thread has its own pace,
/// and on a loaded machine it loses this race.
fn screen_while(session: &Session, what: &str, enough: impl Fn(&Screen) -> bool) -> Screen {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let screen = session.screen();
        if enough(&screen) {
            return screen;
        }
        assert!(
            Instant::now() < deadline,
            "{what} never happened: {:#?}",
            screen.lines
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn screen_until(session: &Session, text: &str) -> Screen {
    screen_while(session, &format!("{text:?} appearing"), |screen| {
        screen.lines.iter().any(|line| line.contains(text))
    })
}

#[test]
fn synthetic_output_ends_with_the_sentinel_and_the_exit_code() {
    let session = harness(&[
        "--synthetic",
        "300000",
        "--sentinel",
        "BENCH-END",
        "--exit-code",
        "7",
    ]);

    screen_until(&session, "BENCH-END");
    assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(7)));
}

#[test]
fn a_recorded_corpus_is_replayed_as_it_was_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus.bin");
    std::fs::write(&corpus, b"\x1b[1mrecorded\x1b[0m session\r\nsecond line").unwrap();

    let session = harness(&["--corpus", corpus.to_str().unwrap(), "--chunk", "3"]);

    let screen = screen_until(&session, "second line");
    assert_eq!(screen.lines[0], "recorded session");
    assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
}

#[test]
fn loops_replay_the_output_again() {
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus.bin");
    std::fs::write(&corpus, b"once\r\n").unwrap();

    let session = harness(&["--corpus", corpus.to_str().unwrap(), "--loops", "3"]);

    assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
    // Waited for, not read once: the program being gone does not mean its last bytes have
    // reached the screen, and reading straight after the exit is a race this test used to
    // lose on a loaded runner.
    screen_while(&session, "three replays arriving", |screen| {
        screen.lines.iter().filter(|line| *line == "once").count() == 3
    });
}

#[test]
fn hooks_run_after_the_replay_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("hooks.log");
    let log_str = log.to_str().unwrap();

    let session = harness(&[
        "--synthetic",
        "1000",
        "--hook",
        &format!("echo notification >> {log_str}"),
        "--hook",
        &format!("echo stop >> {log_str}"),
    ]);

    assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "notification\nstop\n"
    );
}

#[test]
fn a_failing_hook_fails_the_harness() {
    let session = harness(&["--synthetic", "10", "--hook", "exit 4"]);

    assert_ne!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
}

#[test]
fn interactive_mode_answers_each_typed_line_until_quit() {
    let session = harness(&[
        "--synthetic",
        "5000",
        "--sentinel",
        "READY",
        "--interactive",
    ]);
    screen_until(&session, "READY");

    session.write(b"hello there\r").unwrap();
    screen_until(&session, "you said: hello there");

    session.write(b"/quit\r").unwrap();
    assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
}

#[test]
fn waiting_for_input_holds_the_output_back_until_a_line_is_typed() {
    // Plain output: synthetic output can end inside a synchronized update, which the terminal
    // would hold back on its own and make this pass for the wrong reason.
    let dir = tempfile::tempdir().unwrap();
    let corpus = dir.path().join("corpus.bin");
    std::fs::write(&corpus, b"plain output\r\n").unwrap();
    let session = harness(&[
        "--corpus",
        corpus.to_str().unwrap(),
        "--sentinel",
        "AFTER-THE-LINE",
        "--wait-for-input",
    ]);
    // Long enough for the output to have been written, had it not waited.
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !session
            .screen()
            .lines
            .iter()
            .any(|line| line.contains("AFTER-THE-LINE")),
        "the output was written before anything was typed"
    );

    session.write(b"\r").unwrap();

    screen_until(&session, "AFTER-THE-LINE");
    assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
}

#[test]
fn a_corpus_that_does_not_exist_is_an_error_not_silence() {
    let session = harness(&[
        "--corpus",
        Path::new("/definitely/not/here.bin").to_str().unwrap(),
    ]);

    screen_until(&session, "fake-harness:");
    assert_ne!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
}

#[test]
fn the_host_leaves_its_controlling_terminal_and_a_terminal_it_opens_after_is_not_one() {
    // V77: a level-3 agent shares the host's session, so the host has no terminal to share.
    // A program in a pane leads its session and its group, so `setsid` is refused and the
    // host starts again as its own child, which leaves (`charter_core::noterminal`).
    let session = harness(&["--leave-terminal"]);
    let screen = screen_until(&session, "terminal after a pair:");
    let said: Vec<&str> = screen
        .lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("terminal "))
        .collect();
    assert!(said.contains(&"terminal before: yes"), "{said:#?}");
    assert!(
        screen
            .lines
            .iter()
            .any(|line| line.trim() == "acp with a terminal refused: yes"),
        "{:#?}",
        screen.lines
    );
    assert!(said.contains(&"terminal after: no"), "{said:#?}");
    assert!(said.contains(&"terminal after a pair: no"), "{said:#?}");
    assert!(
        !said
            .iter()
            .any(|line| line.starts_with("terminal after") && line.ends_with("yes"))
    );
    assert_eq!(session.wait(PATIENCE).unwrap(), Some(Exit::Code(0)));
}

/// The pid a `--leave-terminal --linger` line names, `<what> pid <n>`, the first time.
fn pid_said(screen: &Screen, what: &str) -> String {
    let prefix = format!("{what} pid ");
    screen
        .lines
        .iter()
        .find_map(|line| line.trim().strip_prefix(&prefix).map(str::to_owned))
        .unwrap_or_else(|| panic!("no {prefix:?} in {:#?}", screen.lines))
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn a_relaunched_host_ends_when_the_process_that_relaunched_it_is_killed() {
    // A SIGKILLed first process can pass nothing on, so the relaunched host watches a pipe from
    // it and ends at its end, rather than living on as an orphaned window.
    let session = harness(&["--leave-terminal", "--linger"]);
    let screen = screen_until(&session, "left as pid");
    let first = pid_said(&screen, "started as");
    let left = pid_said(&screen, "left as");
    assert_ne!(first, left, "the host relaunched itself");
    assert!(alive(&left));
    let killed = std::process::Command::new("kill")
        .args(["-KILL", &first])
        .status()
        .expect("kill runs");
    assert!(killed.success());
    // Well inside the minute `--linger` stays for.
    let deadline = Instant::now() + Duration::from_secs(10);
    while alive(&left) {
        assert!(
            Instant::now() < deadline,
            "the relaunched host {left} outlived its parent"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_relaunched_host_killed_by_a_signal_ends_its_first_process_by_the_same_signal() {
    let session = harness(&["--leave-terminal", "--linger"]);
    let screen = screen_until(&session, "left as pid");
    let left = pid_said(&screen, "left as");
    let killed = std::process::Command::new("kill")
        .args(["-TERM", &left])
        .status()
        .expect("kill runs");
    assert!(killed.success());
    // The operating system's name for it: "Terminated" on Linux, "Terminated: 15" on macOS.
    match session.wait(PATIENCE).unwrap() {
        Some(Exit::Signal(name)) => assert!(name.starts_with("Terminated"), "{name}"),
        other => panic!("ended by {other:?}, not by SIGTERM"),
    }
}

#[test]
fn a_host_that_left_its_terminal_ends_with_a_parent_that_ends_on_ctrl_c() {
    // `cargo tauri dev`: tauri-cli ends on Ctrl-C without ending the app it started. Before
    // the host left its terminal the app got the same Ctrl-C; now it is in a session of its own,
    // so it ends when its parent does.
    let session = harness(&[
        "--parent-of",
        env!("CARGO_BIN_EXE_fake-harness"),
        "--leave-terminal",
        "--linger",
    ]);
    let screen = screen_until(&session, "left as pid");
    let left = pid_said(&screen, "left as");
    assert_eq!(
        pid_said(&screen, "child"),
        left,
        "it left without a relaunch"
    );
    session.write(b"\x03").expect("Ctrl-C");
    let deadline = Instant::now() + Duration::from_secs(10);
    while alive(&left) {
        assert!(
            Instant::now() < deadline,
            "the host {left} outlived its parent"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
