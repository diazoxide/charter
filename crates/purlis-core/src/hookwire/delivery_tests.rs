//! A hook's line is taken by the host or spooled, and the hook knows which before it answers
//! (FD-30, #667).

use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn call(chat: u32, id: &str) -> ToolCall {
    ToolCall {
        chat,
        tool_hook: "posttooluse".to_owned(),
        tool: Some("Read".to_owned()),
        call: Some(id.to_owned()),
        args: None,
        decision: Decision::None,
        rule: None,
        hook_ms: 1,
        agent: None,
        at_ms: 0,
    }
}

fn hearing(tool: Tooled) -> Hearing {
    Hearing {
        blocked: Box::new(|_| {}),
        touching: Box::new(|_| {}),
        each: Box::new(|_| Ok(())),
        answer: Box::new(|_, _| Answer::No { why: String::new() }),
        noticed: Box::new(|_| {}),
        saved: Box::new(|_| {}),
        refused: Box::new(|_| Ok(())),
        tool,
        permission: Box::new(|_| None),
    }
}

fn spooled_calls(socket: &std::path::Path) -> Vec<String> {
    let mut ids = Vec::new();
    spool::drain(&spool::dir_for(socket), &mut |item| {
        if let spool::Drained::Line {
            line: spool::Spooled::Tool(call),
            ..
        } = item
        {
            ids.push(call.call.unwrap_or_default());
        }
        Ok(())
    })
    .expect("the spool drains");
    ids
}

#[test]
fn a_line_the_host_takes_is_answered_only_once_its_hearer_has_recorded_it() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join(".charter/app/hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue(1).expect("a token");
    let recorded = Arc::new(AtomicBool::new(false));
    let _reading = listener.hear(hearing({
        let recorded = Arc::clone(&recorded);
        Box::new(move |_| {
            std::thread::sleep(Duration::from_millis(60));
            recorded.store(true, Ordering::SeqCst);
            Ok(())
        })
    }));

    let delivered = deliver_tool(&path, Some(&token), &call(1, "a")).expect("delivered");

    assert_eq!(delivered, Delivered::Taken);
    assert!(
        recorded.load(Ordering::SeqCst),
        "the host said it took the line before it had recorded it"
    );
    assert_eq!(spooled_calls(&path), Vec::<String>::new());
}

#[test]
fn a_line_no_host_takes_is_spooled_under_the_next_number() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join(".charter/app/hooks.sock");
    let token = {
        // A host issued the token and has gone, as an app that quit has.
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        listener.tokens().issue(2).expect("a token")
    };

    let first = deliver_tool(&path, Some(&token), &call(2, "a")).expect("spooled");
    let second = deliver_tool(&path, Some(&token), &call(2, "b")).expect("spooled");

    assert_eq!(
        (first, second),
        (Delivered::Spooled(1), Delivered::Spooled(2))
    );
    assert_eq!(spooled_calls(&path), ["a", "b"]);
}

#[test]
fn a_host_that_does_not_say_it_took_the_line_in_time_has_it_spooled() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join(".charter/app/hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue(3).expect("a token");
    let _reading = listener.hear(hearing(Box::new(|_| {
        std::thread::sleep(Duration::from_secs(1));
        Ok(())
    })));

    let began = std::time::Instant::now();
    let delivered = deliver_tool(&path, Some(&token), &call(3, "slow")).expect("spooled");

    assert_eq!(delivered, Delivered::Spooled(1));
    assert!(
        began.elapsed() < Duration::from_millis(900),
        "{:?}",
        began.elapsed()
    );
}

#[test]
fn a_line_the_host_could_not_record_is_spooled() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join(".charter/app/hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue(6).expect("a token");
    // The event log's disk is full.
    let _reading = listener.hear(hearing(Box::new(|_| {
        Err(std::io::Error::other("no space left on the device"))
    })));

    let delivered = deliver_tool(&path, Some(&token), &call(6, "full")).expect("spooled");

    assert_eq!(delivered, Delivered::Spooled(1));
    assert_eq!(spooled_calls(&path), ["full"]);
}

#[test]
fn a_line_no_host_takes_outside_the_sandboxs_denial_is_lost_and_says_so() {
    let dir = tempfile::tempdir().expect("a directory");
    // A fallback channel: a socket beside no project's `.charter/app/`.
    let path = dir
        .path()
        .join("charter-op-0123456789abcdef")
        .join("hooks.sock");
    let token = {
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        listener.tokens().issue(7).expect("a token")
    };

    let lost = deliver_tool(&path, Some(&token), &call(7, "a"));

    assert!(lost.is_err(), "{lost:?}");
    assert!(!spool::dir_for(&path).join("7.jsonl").exists());
}

#[test]
fn a_line_with_no_token_and_no_host_is_lost_and_says_so() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join(".charter/app/hooks.sock");

    assert!(deliver_tool(&path, None, &call(4, "a")).is_err());
}

/// The first acceptance line of #667: a host restart during a busy turn loses no event.
#[test]
fn a_host_restart_during_a_busy_turn_loses_no_event() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join(".charter/app/hooks.sock");
    let listener = Listener::bind(dir.path(), &path).expect("a socket");
    let token = listener.tokens().issue(5).expect("a token");
    let heard = Arc::new(Mutex::new(Vec::new()));
    let reading = listener.hear(hearing({
        let heard = Arc::clone(&heard);
        Box::new(move |call: ToolCall| {
            heard.lock().unwrap().push(call.call.unwrap_or_default());
            Ok(())
        })
    }));
    let reading = Mutex::new(Some(reading));

    // Eight hooks at a time, two hundred calls, and the host goes away a third of the way in.
    let next = AtomicUsize::new(0);
    let delivered = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                loop {
                    let n = next.fetch_add(1, Ordering::SeqCst);
                    if n >= 200 {
                        return;
                    }
                    if n == 70 {
                        drop(reading.lock().unwrap().take());
                    }
                    let id = format!("call-{n}");
                    if deliver_tool(&path, Some(&token), &call(5, &id)).is_ok() {
                        delivered.lock().unwrap().push(id);
                    }
                }
            });
        }
    });

    // The next host drains what the hooks spooled while there was none.
    let mut recorded = heard.lock().unwrap().clone();
    recorded.extend(spooled_calls(&path));
    let delivered = delivered.into_inner().unwrap();
    assert_eq!(
        delivered.len(),
        200,
        "every hook either reached the host or spooled"
    );
    for id in &delivered {
        assert!(recorded.contains(id), "{id} was lost");
    }
}
