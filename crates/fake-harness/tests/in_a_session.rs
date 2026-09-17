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

fn screen_until(session: &Session, text: &str) -> Screen {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let screen = session.screen();
        if screen.lines.iter().any(|line| line.contains(text)) {
            return screen;
        }
        assert!(
            Instant::now() < deadline,
            "{text:?} never appeared: {:#?}",
            screen.lines
        );
        std::thread::sleep(Duration::from_millis(10));
    }
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
    let screen = session.screen();
    assert_eq!(
        screen.lines.iter().filter(|line| *line == "once").count(),
        3
    );
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
