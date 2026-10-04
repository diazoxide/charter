//! An ACP agent that stops reading its stdin while it goes on sending lines that earn answers
//! (HP-2 review): whatever the lines' shape, the answers charter cannot write are bounded in
//! bytes, and past the bound the chat ends.
#![cfg(unix)]

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use charter_core::acp::{Chat, Event, Launch, Stop, TurnFailed};
use charter_core::harness::asks::Asks;

const PATIENCE: Duration = Duration::from_secs(30);

/// What this test process's memory may grow by while a chat floods it: the unwritten bound, the
/// lines in flight, and room for the allocator.
const MOST_GROWTH_KIB: u64 = 96 * 1024;

/// One flood at a time, so they do not compete for the machine.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Set on the copy of this binary each test runs its flood in.
const ALONE: &str = "CHARTER_UNREAD_ANSWERS_ALONE";

/// Runs test `name` again in a process of its own and checks it passed there. Memory a flood
/// freed stays resident in this process, so a second flood here would grow into it unseen; each
/// measures its own growth only where it is the first.
fn in_a_process_of_its_own(name: &str, shape: &str, count: u64, bytes: usize) {
    if std::env::var_os(ALONE).is_some() {
        ends_bounded(shape, count, bytes);
        return;
    }
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let status = std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args([name, "--exact", "--test-threads=1"])
        .env(ALONE, "1")
        .status()
        .expect("the test runs again");
    assert!(
        status.success(),
        "{name} failed in its own process: {status}"
    );
}

fn resident_kib() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("a size in KiB")
}

fn launch(dir: &Path) -> Launch {
    static LEFT: std::sync::Once = std::sync::Once::new();
    LEFT.call_once(|| {
        use charter_core::noterminal::{Left, leave};
        if let Left::Relaunched(code) = leave().expect("the tests leave their terminal") {
            std::process::exit(code);
        }
    });
    Launch {
        chat: "chat-1".to_owned(),
        argv: vec![
            env!("CARGO_BIN_EXE_fake-harness").to_owned(),
            "--acp".to_owned(),
        ],
        cwd: dir.to_path_buf(),
        env: Vec::new(),
        charter_mcp: None,
        patience: PATIENCE,
    }
}

/// Floods with `unread <shape> <count> <bytes>` and checks the chat ends with this process's
/// memory grown by no more than [`MOST_GROWTH_KIB`].
fn ends_bounded(shape: &str, count: u64, bytes: usize) {
    let dir = tempfile::tempdir().expect("a worktree");
    let before = resident_kib();
    let (chat, events) = Chat::start(launch(dir.path()), Arc::new(Asks::new())).expect("starts");
    let chat = Arc::new(chat);
    let turn: Receiver<Result<Stop, TurnFailed>> = {
        let (ended, turn) = std::sync::mpsc::channel();
        let (chat, text) = (Arc::clone(&chat), format!("unread {shape} {count} {bytes}"));
        std::thread::spawn(move || ended.send(chat.prompt(&text)));
        turn
    };
    let mut most = before;
    let deadline = std::time::Instant::now() + PATIENCE;
    let ended = loop {
        assert!(
            std::time::Instant::now() < deadline,
            "{shape}: the chat never ended"
        );
        most = most.max(resident_kib());
        match turn.recv_timeout(Duration::from_millis(50)) {
            Ok(ended) => break ended,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(err) => panic!("{err}"),
        }
        // Stop a flood that is not ended long before it takes the machine with it.
        assert!(
            most.saturating_sub(before) < MOST_GROWTH_KIB * 4,
            "{shape}: grew by {} KiB and still going",
            most.saturating_sub(before)
        );
    };
    let grew = most.saturating_sub(before);
    assert_eq!(ended, Err(TurnFailed::Gone), "{shape}");
    loop {
        match events.recv_timeout(PATIENCE).expect("an event") {
            Event::Ended => break,
            _ => continue,
        }
    }
    assert!(grew < MOST_GROWTH_KIB, "{shape}: grew by {grew} KiB");
}

#[test]
fn lines_that_are_not_json_end_the_chat_once_their_answers_back_up() {
    in_a_process_of_its_own(
        "lines_that_are_not_json_end_the_chat_once_their_answers_back_up",
        "garbage",
        400,
        1 << 20,
    );
}

#[test]
fn tiny_lines_that_are_no_message_end_the_chat_once_their_answers_back_up() {
    in_a_process_of_its_own(
        "tiny_lines_that_are_no_message_end_the_chat_once_their_answers_back_up",
        "tiny",
        2_000_000,
        0,
    );
}

#[test]
fn requests_with_a_key_twice_end_the_chat_once_their_answers_back_up() {
    in_a_process_of_its_own(
        "requests_with_a_key_twice_end_the_chat_once_their_answers_back_up",
        "dupkey",
        200,
        1 << 20,
    );
}

#[test]
fn requests_with_a_null_id_end_the_chat_once_their_answers_back_up() {
    in_a_process_of_its_own(
        "requests_with_a_null_id_end_the_chat_once_their_answers_back_up",
        "nullid",
        1_000_000,
        0,
    );
}

#[test]
fn batches_with_a_bad_member_end_the_chat_once_their_answers_back_up() {
    in_a_process_of_its_own(
        "batches_with_a_bad_member_end_the_chat_once_their_answers_back_up",
        "badbatch",
        400,
        1 << 20,
    );
}

#[test]
fn requests_with_trailing_garbage_end_the_chat_once_their_answers_back_up() {
    in_a_process_of_its_own(
        "requests_with_trailing_garbage_end_the_chat_once_their_answers_back_up",
        "trailing",
        400,
        1 << 20,
    );
}

#[test]
fn requests_with_huge_ids_end_the_chat_once_their_answers_back_up() {
    in_a_process_of_its_own(
        "requests_with_huge_ids_end_the_chat_once_their_answers_back_up",
        "hugeid",
        100,
        4 << 20,
    );
}
