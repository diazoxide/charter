use super::*;

const DEVICE: &str = "01J9ZZDEVICE0000000000000";

#[test]
fn an_event_is_one_line_in_adr_0066s_envelope_numbered_from_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = Log::open(dir.path(), DEVICE).unwrap();

    let first = log
        .append(
            Some("CHAT"),
            Some("RUN"),
            None,
            "hook.stop",
            serde_json::json!({}),
        )
        .unwrap();
    let second = log
        .append(
            Some("CHAT"),
            Some("RUN"),
            None,
            "hook.stop",
            serde_json::json!({}),
        )
        .unwrap();

    assert_eq!((first.seq, second.seq), (1, 2));
    assert_eq!(first.v, 1);
    assert_eq!(first.device_id, DEVICE);
    assert_eq!(first.ulid.len(), 26, "a ULID: {}", first.ulid);
    assert_ne!(first.ulid, second.ulid);
    assert_eq!(read(dir.path()).unwrap(), vec![first, second]);
}

#[test]
fn a_log_opened_again_carries_on_from_its_last_number() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut log = Log::open(dir.path(), DEVICE).unwrap();
        for _ in 0..3 {
            log.append(
                Some("CHAT"),
                None,
                None,
                "chat.opened",
                serde_json::json!({}),
            )
            .unwrap();
        }
    }

    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    let next = log
        .append(
            Some("CHAT"),
            None,
            None,
            "chat.opened",
            serde_json::json!({}),
        )
        .unwrap();

    assert_eq!(next.seq, 4, "a seq is never reused");
}

#[test]
fn a_second_writer_on_the_same_log_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let _first = Log::open(dir.path(), DEVICE).unwrap();

    let second = Log::open(dir.path(), DEVICE);

    assert!(second.is_err(), "one writer per device (ADR 0068)");
}

/// #972: a host that exits never unlocks the log's lock, so it is let go of only when the last
/// copy of its descriptor closes — and a child the host forked holds one until it execs. A log
/// opened again in that moment, as when the app is relaunched, waits the child out instead of
/// being refused as a second writer. The host here is a plain locked file dropped without an
/// unlock, and the child keeps a copy as its stdin for as long as it sleeps.
#[cfg(unix)]
#[test]
fn a_log_opened_again_while_a_child_of_an_exited_host_still_holds_its_lock_waits_the_child_out() {
    let dir = tempfile::tempdir().unwrap();
    crate::secrets::make_private_dir(dir.path()).unwrap();
    let host = private_file(&dir.path().join(LOCK)).unwrap();
    host.lock().unwrap();
    let copy = std::process::Stdio::from(host.try_clone().unwrap());
    let mut child =
        crate::forklock::spawn(std::process::Command::new("sleep").arg("0.4").stdin(copy)).unwrap();

    drop(host);
    let asked = Instant::now();
    let again = Log::open(dir.path(), DEVICE);
    let waited = asked.elapsed();
    child.wait().unwrap();

    assert!(
        again.is_ok(),
        "a child holding the lock a moment is not a second writer: {:?}",
        again.err()
    );
    assert!(
        waited >= std::time::Duration::from_millis(100),
        "the lock was free at once ({waited:?}), so nothing here was waited out"
    );
}

/// #1316: a child that keeps a copy of the lock's descriptor for longer than [`LOCK_WAIT`] —
/// here as its stdin, for as long as it sleeps — does not keep the next writer out once the
/// log that held the lock is dropped, since the drop unlocks every copy at once.
#[cfg(unix)]
#[test]
fn a_log_dropped_lets_go_of_its_lock_though_a_child_keeps_a_copy_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::open(dir.path(), DEVICE).unwrap();
    let copy = std::process::Stdio::from(log._lock.try_clone().unwrap());
    let mut child =
        crate::forklock::spawn(std::process::Command::new("sleep").arg("30").stdin(copy)).unwrap();

    drop(log);
    let again = Log::open(dir.path(), DEVICE);
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        again.is_ok(),
        "the lock outlived its log: {:?}",
        again.err()
    );
}

#[test]
fn an_args_digest_is_keyed_by_the_device_and_stays_the_same_across_launches() {
    let one = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let args = args_hash(&serde_json::json!({"command": "ls -la"}));

    let here = ArgsKey::open(one.path()).unwrap().digest(&args);
    let again = ArgsKey::open(one.path()).unwrap().digest(&args);
    let elsewhere = ArgsKey::open(other.path()).unwrap().digest(&args);

    assert_eq!(
        here, again,
        "the key is kept, so a digest can be compared with yesterday's"
    );
    assert_ne!(here, elsewhere, "another device's key gives another digest");
    assert_ne!(
        here, args,
        "the log never holds the plain hash, which a guess could match"
    );
    assert_eq!(here.len(), 64);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(one.path().join(ARGS_KEY))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "the key is the operator's alone");
    }
}

#[test]
fn the_same_arguments_hash_the_same_and_different_ones_do_not() {
    let a = args_hash(&serde_json::json!({"command": "ls"}));
    assert_eq!(a, args_hash(&serde_json::json!({"command": "ls"})));
    assert_ne!(a, args_hash(&serde_json::json!({"command": "ls -la"})));
    assert_eq!(a.len(), 64, "sha-256, hex");
}

fn report(chat: u32, event: crate::state::Event) -> crate::hookwire::Report {
    crate::hookwire::Report {
        chat,
        event,
        conversation: crate::hookwire::Conversation::default(),
        pid: None,
        agent: None,
        detail: crate::state::Detail::default(),
    }
}

fn recorder(dir: &Path) -> Recorder {
    Recorder::new(Log::open(dir, DEVICE).unwrap(), ArgsKey::open(dir).unwrap())
}

#[test]
fn events_of_two_runs_in_one_chat_group_under_that_chat() {
    use crate::state::Event::{SessionStart, Stop, UserPromptSubmit};
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/planes/one");
    let mut host = recorder(dir.path());

    host.report(plane, &report(7, SessionStart), Followed::No)
        .unwrap();
    host.report(plane, &report(7, UserPromptSubmit), Followed::No)
        .unwrap();
    host.report(plane, &report(7, Stop), Followed::No).unwrap();
    // `/clear`: the board follows the chat onto another conversation, which is a new run.
    host.report(plane, &report(7, SessionStart), Followed::Moved)
        .unwrap();
    host.report(plane, &report(7, UserPromptSubmit), Followed::No)
        .unwrap();

    let events = read(dir.path()).unwrap();
    let chats: std::collections::BTreeSet<_> = events.iter().map(|e| e.chat.clone()).collect();
    assert_eq!(chats.len(), 1, "one chat: {events:#?}");
    let runs: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "run.started")
        .map(|e| (e.run.clone().unwrap(), e.body["cause"].clone()))
        .collect();
    assert_eq!(runs.len(), 2, "{events:#?}");
    assert_ne!(runs[0].0, runs[1].0, "two runs");
    assert_eq!(runs[0].1, "start");
    assert_eq!(runs[1].1, "clear");
    let hooks: Vec<_> = events
        .iter()
        .filter(|e| e.kind.starts_with("hook."))
        .map(|e| (e.kind.as_str(), e.run.clone().unwrap()))
        .collect();
    assert_eq!(
        hooks,
        vec![
            ("hook.sessionstart", runs[0].0.clone()),
            ("hook.userpromptsubmit", runs[0].0.clone()),
            ("hook.stop", runs[0].0.clone()),
            ("hook.sessionstart", runs[1].0.clone()),
            ("hook.userpromptsubmit", runs[1].0.clone()),
        ],
        "every hook call is one event, under the run it happened in"
    );
}

#[test]
fn a_conversation_named_for_the_first_time_and_a_compaction_keep_the_run() {
    use crate::state::Event::SessionStart;
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/planes/one");
    let mut host = recorder(dir.path());

    host.report(plane, &report(1, SessionStart), Followed::No)
        .unwrap();
    host.report(plane, &report(1, SessionStart), Followed::FirstNamed)
        .unwrap();

    let events = read(dir.path()).unwrap();
    assert_eq!(
        events.iter().filter(|e| e.kind == "run.started").count(),
        1,
        "ADR 0066: a harness naming its conversation keeps the run: {events:#?}"
    );
}

#[test]
fn the_same_number_in_two_planes_is_two_chats() {
    use crate::state::Event::Stop;
    let dir = tempfile::tempdir().unwrap();
    let mut host = recorder(dir.path());

    host.report(Path::new("/planes/one"), &report(1, Stop), Followed::No)
        .unwrap();
    host.report(Path::new("/planes/two"), &report(1, Stop), Followed::No)
        .unwrap();

    let events = read(dir.path()).unwrap();
    let chats: std::collections::BTreeSet<_> = events.iter().map(|e| e.chat.clone()).collect();
    assert_eq!(
        chats.len(),
        2,
        "a chat's number means nothing outside its plane"
    );
}

fn tool_call(
    chat: u32,
    word: &str,
    decision: crate::hookwire::Decision,
) -> crate::hookwire::ToolCall {
    crate::hookwire::ToolCall {
        chat,
        tool_hook: word.to_owned(),
        tool: Some("Bash".to_owned()),
        call: Some("toolu_01".to_owned()),
        args: Some(args_hash(&serde_json::json!({"command": "ls"}))),
        decision,
        rule: (decision == crate::hookwire::Decision::Deny).then(|| "no-force-push".to_owned()),
        hook_ms: 3,
        agent: None,
        at_ms: 0,
    }
}

#[test]
fn a_tool_call_is_one_event_with_its_tool_keyed_digest_decision_and_durations() {
    use crate::hookwire::Decision;
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/planes/one");
    let mut host = recorder(dir.path());
    let at = Instant::now();

    let pre = host
        .tool(plane, &tool_call(4, "pretooluse", Decision::Deny), at)
        .unwrap();
    let post = host
        .tool(
            plane,
            &tool_call(4, "posttooluse", Decision::None),
            at + Duration::from_millis(250),
        )
        .unwrap();

    assert_eq!(pre.kind, "hook.pretooluse");
    assert_eq!(pre.body["tool"], "Bash");
    assert_eq!(pre.body["call"], "toolu_01");
    assert_eq!(pre.body["decision"], "deny");
    assert_eq!(pre.body["rule"], "no-force-push");
    assert_eq!(pre.body["hook_ms"], 3);
    let sent = args_hash(&serde_json::json!({"command": "ls"}));
    let kept = pre.body["args"].as_str().unwrap();
    assert_eq!(
        kept,
        ArgsKey::open(dir.path()).unwrap().digest(&sent),
        "keyed"
    );
    assert_ne!(kept, sent, "never the hash the hook sent");
    assert_eq!(post.kind, "hook.posttooluse");
    assert_eq!(
        post.body["tool_ms"], 250,
        "from the call's pre hook to its post hook"
    );
    assert_eq!(pre.run, post.run, "both under the chat's current run");
    let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
    assert!(!text.contains("ls"), "no argument reaches the log: {text}");
}

#[test]
fn the_host_keeps_the_log_under_the_data_home_by_device_id() {
    use crate::state::Event::Stop;
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let data = dir.path().join("data");

    let mut host = Recorder::open_in(&config, &data).unwrap();
    host.report(Path::new("/p"), &report(1, Stop), Followed::No)
        .unwrap();

    let device = crate::machine::device_id(&config).unwrap();
    let events = read(&data.join("events").join(&device)).unwrap();
    assert_eq!(events.len(), 2, "run.started and hook.stop: {events:#?}");
    assert!(events.iter().all(|e| e.device_id == device));
}

#[test]
fn a_data_home_inside_a_repository_gets_no_log() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(dir.path().join("repo").join(".git")).unwrap();

    let refused = Recorder::open_in(&config, &dir.path().join("repo").join("data"));

    assert!(refused.is_err());
    assert!(
        !dir.path().join("repo").join("data").exists(),
        "nothing was made there"
    );
}

#[test]
fn a_line_torn_by_a_crash_is_cut_off_and_the_next_event_is_whole() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut log = Log::open(dir.path(), DEVICE).unwrap();
        log.append(
            Some("CHAT"),
            None,
            None,
            "chat.opened",
            serde_json::json!({}),
        )
        .unwrap();
    }
    // The machine died in the middle of the second line.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join(FILE))
        .unwrap();
    std::io::Write::write_all(&mut file, br#"{"v":1,"device_id":"X","seq":2,"ul"#).unwrap();
    drop(file);

    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    let next = log
        .append(
            Some("CHAT"),
            None,
            None,
            "chat.opened",
            serde_json::json!({}),
        )
        .unwrap();

    let events = read(dir.path()).unwrap();
    assert_eq!(
        events.len(),
        2,
        "the torn line went, the new one is whole: {events:#?}"
    );
    assert_eq!(events[0].seq, 1);
    assert_eq!(next.seq, 2);
    assert_eq!(events[1], next);
}

#[test]
fn a_log_with_lines_but_no_number_in_them_is_refused_rather_than_counted_from_one_again() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE), "not an event\nnor this\n").unwrap();

    let refused = Log::open(dir.path(), DEVICE);

    assert!(
        refused.is_err(),
        "a seq is never reused, so a log it cannot read is not restarted"
    );
}

#[test]
fn a_last_seq_at_the_top_of_the_range_is_refused_rather_than_wrapped() {
    let dir = tempfile::tempdir().unwrap();
    let event = Event {
        v: VERSION,
        device_id: DEVICE.to_owned(),
        seq: u64::MAX,
        ulid: ulid::Ulid::generate().to_string(),
        chat: None,
        run: None,
        parent_run: None,
        kind: "chat.opened".to_owned(),
        body: serde_json::json!({}),
    };
    std::fs::write(
        dir.path().join(FILE),
        format!("{}\n", serde_json::to_string(&event).unwrap()),
    )
    .unwrap();

    let opened = Log::open(dir.path(), DEVICE);

    assert!(opened.is_err());
}

#[test]
fn a_chat_the_host_has_not_seen_begins_with_start_even_when_its_first_line_is_a_move() {
    use crate::state::Event::SessionStart;
    let dir = tempfile::tempdir().unwrap();
    let mut host = recorder(dir.path());

    host.report(Path::new("/p"), &report(2, SessionStart), Followed::Moved)
        .unwrap();

    let events = read(dir.path()).unwrap();
    assert_eq!(events[0].kind, "run.started");
    assert_eq!(
        events[0].body["cause"], "start",
        "there was no run to clear: {events:#?}"
    );
}

#[test]
fn a_sub_agents_calls_are_a_child_run_of_the_run_that_was_current_until_its_stop() {
    use crate::hookwire::Decision;
    use std::time::Instant;
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/p");
    let mut host = recorder(dir.path());
    let parent = host
        .report(
            plane,
            &report(5, crate::state::Event::UserPromptSubmit),
            Followed::No,
        )
        .unwrap()
        .run
        .unwrap();
    let mut by_agent = tool_call(5, "pretooluse", Decision::None);
    by_agent.agent = Some("agent-1".to_owned());

    let first = host.tool(plane, &by_agent, Instant::now()).unwrap();
    let second = host.tool(plane, &by_agent, Instant::now()).unwrap();
    let mut stop = report(5, crate::state::Event::SubagentStop);
    stop.agent = Some("agent-1".to_owned());
    let stopped = host.report(plane, &stop, Followed::No).unwrap();
    let after = host.tool(plane, &by_agent, Instant::now()).unwrap();

    let child = first.run.clone().unwrap();
    assert_ne!(child, parent, "a sub-agent is a run of its own");
    assert_eq!(first.parent_run.as_deref(), Some(parent.as_str()));
    assert_eq!(
        second.run, first.run,
        "the same agent is the same child run"
    );
    assert_eq!(stopped.run, first.run, "its stop is its own");
    assert_eq!(
        after.run.as_deref(),
        Some(parent.as_str()),
        "a stopped agent's late line is the parent run's, and mints no second child run"
    );
    assert_eq!(after.parent_run, None);
    let started: Vec<_> = read(dir.path())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "run.started")
        .collect();
    assert_eq!(
        started.len(),
        2,
        "the parent's run and one child run: {started:#?}"
    );
    assert_eq!(started[1].body["cause"], "child");
    assert_eq!(started[1].parent_run.as_deref(), Some(parent.as_str()));
}

#[test]
fn a_tool_hook_word_charter_does_not_answer_is_recorded_under_one_kind() {
    use crate::hookwire::Decision;
    let dir = tempfile::tempdir().unwrap();
    let mut host = recorder(dir.path());

    let forged = host
        .tool(
            Path::new("/p"),
            &tool_call(1, "chat.closed", Decision::None),
            std::time::Instant::now(),
        )
        .unwrap();

    assert_eq!(
        forged.kind, "hook.unknown",
        "no line can mint a kind of its own"
    );
    assert_eq!(forged.body["word"], "chat.closed");
}

#[test]
fn a_tool_calls_duration_is_taken_once_and_a_call_left_open_is_let_go_by_age() {
    use crate::hookwire::Decision;
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/p");
    let mut host = recorder(dir.path());
    let at = Instant::now();

    host.tool(plane, &tool_call(1, "pretooluse", Decision::None), at)
        .unwrap();
    let post = host
        .tool(
            plane,
            &tool_call(1, "posttooluse", Decision::None),
            at + Duration::from_millis(5),
        )
        .unwrap();
    let again = host
        .tool(
            plane,
            &tool_call(1, "posttooluse-skill", Decision::None),
            at + Duration::from_millis(9),
        )
        .unwrap();
    assert_eq!(post.body["tool_ms"], 5);
    assert!(
        again.body.get("tool_ms").is_none(),
        "a call's duration is taken once"
    );

    let mut stale = tool_call(1, "pretooluse", Decision::None);
    stale.call = Some("toolu_old".to_owned());
    host.tool(plane, &stale, at).unwrap();
    // Long after: the pre hook's entry is older than a call is ever left open.
    let late = at + CALL_IS_OPEN_AT_MOST + Duration::from_secs(1);
    host.tool(plane, &tool_call(1, "pretooluse", Decision::None), late)
        .unwrap();
    let mut stale_post = tool_call(1, "posttooluse", Decision::None);
    stale_post.call = Some("toolu_old".to_owned());
    let stale_post = host.tool(plane, &stale_post, late).unwrap();
    assert!(
        stale_post.body.get("tool_ms").is_none(),
        "the open call was let go by age"
    );
}

#[test]
fn a_sub_agents_child_run_ends_with_its_parents_run() {
    use crate::hookwire::Decision;
    use crate::state::Event::{SessionEnd, SessionStart, UserPromptSubmit};
    use std::time::Instant;
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/p");
    let mut host = recorder(dir.path());
    host.report(plane, &report(5, UserPromptSubmit), Followed::No)
        .unwrap();
    let mut by_agent = tool_call(5, "pretooluse", Decision::None);
    by_agent.agent = Some("agent-1".to_owned());
    let before = host.tool(plane, &by_agent, Instant::now()).unwrap();

    // `/clear`: the parent run ends, and a Codex sub-agent, whose `SubagentStop` charter does
    // not arm, ends with it.
    host.report(plane, &report(5, SessionStart), Followed::Moved)
        .unwrap();
    let after_clear = host.tool(plane, &by_agent, Instant::now()).unwrap();
    host.report(plane, &report(5, SessionEnd), Followed::No)
        .unwrap();
    let after_end = host.tool(plane, &by_agent, Instant::now()).unwrap();

    assert_ne!(
        after_clear.run, before.run,
        "the child of the cleared run ended with it"
    );
    assert_ne!(
        after_clear.parent_run, before.parent_run,
        "a child of the new run"
    );
    assert_ne!(
        after_end.run, after_clear.run,
        "a child run ends with its parent's session"
    );
}

#[test]
fn a_tool_calls_post_hook_heard_before_its_pre_hook_still_pairs_by_when_each_ran() {
    use crate::hookwire::Decision;
    use std::time::Instant;
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/p");
    let mut host = recorder(dir.path());
    let mut pre = tool_call(1, "pretooluse", Decision::None);
    pre.at_ms = 10_000;
    let mut post = tool_call(1, "posttooluse", Decision::None);
    post.at_ms = 10_420;

    let post_first = host.tool(plane, &post, Instant::now()).unwrap();
    let pre_second = host.tool(plane, &pre, Instant::now()).unwrap();

    assert!(
        post_first.body.get("tool_ms").is_none(),
        "its pre hook is not heard yet"
    );
    assert_eq!(
        pre_second.body["tool_ms"], 420,
        "the second of the pair says how long the tool ran, by the hooks' own clocks"
    );
}

#[test]
fn a_post_hook_heard_long_after_its_pre_hook_gives_no_duration() {
    use crate::hookwire::Decision;
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/p");
    let mut host = recorder(dir.path());
    let at = Instant::now();

    host.tool(plane, &tool_call(1, "pretooluse", Decision::None), at)
        .unwrap();
    let late = host
        .tool(
            plane,
            &tool_call(1, "posttooluse", Decision::None),
            at + CALL_IS_OPEN_AT_MOST + Duration::from_secs(1),
        )
        .unwrap();

    assert!(
        late.body.get("tool_ms").is_none(),
        "a call is open an hour at most"
    );
}

#[test]
fn a_write_that_failed_partway_is_cut_back_before_the_next_event() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    log.append(
        Some("CHAT"),
        None,
        None,
        "chat.opened",
        serde_json::json!({}),
    )
    .unwrap();

    log.fail_the_next_write_after(10);
    assert!(
        log.append(
            Some("CHAT"),
            None,
            None,
            "chat.opened",
            serde_json::json!({})
        )
        .is_err()
    );
    let next = log
        .append(
            Some("CHAT"),
            None,
            None,
            "chat.opened",
            serde_json::json!({}),
        )
        .unwrap();

    let events = read(dir.path()).unwrap();
    assert_eq!(events.len(), 2, "the half line is gone: {events:#?}");
    assert_eq!(events[1], next);
    let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
}

use crate::reopen::tests::{CHAT, RUN};

#[test]
fn a_chat_reopened_after_a_relaunch_keeps_its_id_and_its_first_event_is_a_reopen_run() {
    use crate::state::Event::{SessionStart, Stop};
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/planes/one");
    {
        let mut before = recorder(dir.path());
        before
            .begin(
                plane,
                7,
                RunOf {
                    chat: CHAT,
                    run: RUN,
                },
                Began::Start,
            )
            .unwrap();
        before
            .report(plane, &report(7, Stop), Followed::No)
            .unwrap();
    }
    let relaunched_at = read(dir.path()).unwrap().len();

    // The next launch: a host with nothing in its memory, told by the record who chat 7 is.
    let mut after = recorder(dir.path());
    let again = "01K6E8ZK6V4Q9T0N3M2B1C5D7K";
    after
        .begin(
            plane,
            7,
            RunOf {
                chat: CHAT,
                run: again,
            },
            Began::Reopen,
        )
        .unwrap();
    after
        .report(plane, &report(7, SessionStart), Followed::No)
        .unwrap();

    let events = read(dir.path()).unwrap();
    let since: Vec<_> = events[relaunched_at..]
        .iter()
        .map(|e| {
            (
                e.kind.as_str(),
                e.chat.as_deref(),
                e.run.as_deref(),
                e.body.get("cause").and_then(|c| c.as_str()),
            )
        })
        .collect();
    assert_eq!(
        since,
        vec![
            ("run.started", Some(CHAT), Some(again), Some("reopen")),
            ("hook.sessionstart", Some(CHAT), Some(again), None),
        ],
        "{events:#?}"
    );
    assert_eq!(
        events[0].chat.as_deref(),
        Some(CHAT),
        "the same chat as before"
    );
}

#[test]
fn fresh_and_switch_each_begin_a_run_in_the_same_chat() {
    use crate::state::Event::Stop;
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/planes/one");
    let mut host = recorder(dir.path());
    let runs = [
        "01K6E8ZK6V4Q9T0N3M2B1C5D70",
        "01K6E8ZK6V4Q9T0N3M2B1C5D71",
        "01K6E8ZK6V4Q9T0N3M2B1C5D72",
    ];

    host.begin(
        plane,
        3,
        RunOf {
            chat: CHAT,
            run: runs[0],
        },
        Began::Start,
    )
    .unwrap();
    host.begin(
        plane,
        3,
        RunOf {
            chat: CHAT,
            run: runs[1],
        },
        Began::Fresh,
    )
    .unwrap();
    host.report(plane, &report(3, Stop), Followed::No).unwrap();
    host.begin(
        plane,
        3,
        RunOf {
            chat: CHAT,
            run: runs[2],
        },
        Began::Switch,
    )
    .unwrap();
    host.report(plane, &report(3, Stop), Followed::No).unwrap();

    let events = read(dir.path()).unwrap();
    let said: Vec<_> = events
        .iter()
        .map(|e| {
            (
                e.chat.as_deref().unwrap(),
                e.run.as_deref().unwrap(),
                e.body
                    .get("cause")
                    .and_then(|c| c.as_str())
                    .unwrap_or(&e.kind),
            )
        })
        .collect();
    assert_eq!(
        said,
        vec![
            (CHAT, runs[0], "start"),
            (CHAT, runs[1], "fresh"),
            (CHAT, runs[1], "hook.stop"),
            (CHAT, runs[2], "switch"),
            (CHAT, runs[2], "hook.stop"),
        ]
    );
}

#[test]
fn a_new_run_ends_the_sub_agents_of_the_one_before() {
    use crate::state::Event::Stop;
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/planes/one");
    let mut host = recorder(dir.path());
    let agent = |chat| crate::hookwire::Report {
        agent: Some("a1".to_owned()),
        ..report(chat, Stop)
    };

    host.begin(
        plane,
        3,
        RunOf {
            chat: CHAT,
            run: RUN,
        },
        Began::Start,
    )
    .unwrap();
    host.report(plane, &agent(3), Followed::No).unwrap();
    let next = "01K6E8ZK6V4Q9T0N3M2B1C5D7K";
    host.begin(
        plane,
        3,
        RunOf {
            chat: CHAT,
            run: next,
        },
        Began::Switch,
    )
    .unwrap();
    let after = host.report(plane, &agent(3), Followed::No).unwrap();

    assert_eq!(
        after.parent_run.as_deref(),
        Some(next),
        "the agent seen again is a child of the run now current, not of the one that ended"
    );
}

#[test]
fn a_chat_put_back_in_its_conversation_is_a_reopen_and_one_put_back_without_it_is_fresh() {
    use crate::harness::SessionId;
    use crate::reopen::{Fresh, Reopened};
    let resumed =
        Reopened::Resumed(SessionId::new("11111111-2222-4333-8444-555555555555").unwrap());
    let on = |how: Reopened, harness| Began::at_relaunch(&how, harness);

    assert_eq!(on(resumed, true), Began::Reopen);
    assert_eq!(
        on(Reopened::Fresh(Fresh::SessionNamedByTheOperator), true),
        Began::Reopen,
        "the operator's own arguments resume it"
    );
    assert_eq!(
        on(Reopened::Fresh(Fresh::NoResumeForThisProgram), false),
        Began::Reopen,
        "a shell has no conversation to lose"
    );
    for lost in [
        Fresh::WorkspaceRenamed,
        Fresh::NoConversationRecorded,
        Fresh::NoResumeForThisProgram,
    ] {
        assert_eq!(on(Reopened::Fresh(lost), true), Began::Fresh, "{lost:?}");
    }
}

#[test]
fn a_run_the_log_could_not_write_still_keeps_the_chat_under_its_id() {
    use crate::state::Event::Stop;
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/planes/one");
    let mut host = recorder(dir.path());

    host.log.fail_the_next_write_after(3);
    assert!(
        host.begin(
            plane,
            3,
            RunOf {
                chat: CHAT,
                run: RUN
            },
            Began::Reopen
        )
        .is_err()
    );
    let next = host.report(plane, &report(3, Stop), Followed::No).unwrap();

    assert_eq!(
        (next.chat.as_deref(), next.run.as_deref()),
        (Some(CHAT), Some(RUN)),
        "not a second id minted at the next line"
    );
}

#[test]
fn a_clear_begins_the_run_the_host_minted_for_it() {
    use crate::state::Event::{SessionStart, Stop};
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/planes/one");
    let mut host = recorder(dir.path());
    host.begin(
        plane,
        3,
        RunOf {
            chat: CHAT,
            run: RUN,
        },
        Began::Start,
    )
    .unwrap();

    let cleared = "01K6E8ZK6V4Q9T0N3M2B1C5D7K";
    let event = host
        .report_with(
            plane,
            &report(3, SessionStart),
            Followed::Moved,
            Some(cleared),
        )
        .unwrap();
    host.report(plane, &report(3, Stop), Followed::No).unwrap();

    assert_eq!(event.run.as_deref(), Some(cleared));
    let started: Vec<_> = read(dir.path())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "run.started")
        .map(|e| (e.run.unwrap(), e.body["cause"].as_str().unwrap().to_owned()))
        .collect();
    assert_eq!(
        started,
        vec![
            (RUN.to_owned(), "start".to_owned()),
            (cleared.to_owned(), "clear".to_owned())
        ]
    );
}

/// Appends `n` events to `log`, answering their seqs.
fn append_n(log: &mut Log, n: usize) -> Vec<u64> {
    (0..n)
        .map(|_| {
            log.append(
                Some("CHAT"),
                Some("RUN"),
                None,
                "hook.stop",
                serde_json::json!({}),
            )
            .unwrap()
            .seq
        })
        .collect()
}

/// The seqs of the events in `got`.
fn seqs(got: &[Delivery]) -> Vec<u64> {
    got.iter()
        .filter_map(|delivery| match delivery {
            Delivery::Event(event) => Some(event.seq),
            Delivery::Missed { .. } => None,
        })
        .collect()
}

#[test]
fn a_subscriber_gets_every_event_after_its_cursor_in_order_and_then_what_is_written_next() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    append_n(&mut log, 5);

    let mut subscription = subscribe(dir.path(), 2);
    let first = subscription.poll().unwrap();
    append_n(&mut log, 2);
    let second = subscription.poll().unwrap();
    let idle = subscription.poll().unwrap();

    assert_eq!(seqs(&first), vec![3, 4, 5]);
    assert_eq!(seqs(&second), vec![6, 7], "the stream stays open");
    assert!(idle.is_empty());
    assert_eq!(subscription.cursor(), 7);
}

#[test]
fn a_line_the_writer_died_in_is_never_delivered_and_its_seq_comes_once_from_the_next_writer() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut log = Log::open(dir.path(), DEVICE).unwrap();
        append_n(&mut log, 3);
    }
    let mut subscription = subscribe(dir.path(), 0);
    assert_eq!(seqs(&subscription.poll().unwrap()), vec![1, 2, 3]);
    // The host died in the middle of the fourth line.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join(FILE))
        .unwrap();
    std::io::Write::write_all(&mut file, br#"{"v":1,"device_id":"X","seq":4,"ul"#).unwrap();
    drop(file);
    assert!(
        subscription.poll().unwrap().is_empty(),
        "half a line is no event"
    );

    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    append_n(&mut log, 2);
    let resubscribed = subscribe(dir.path(), subscription.cursor()).poll().unwrap();
    let carried_on = subscription.poll().unwrap();

    assert_eq!(seqs(&resubscribed), vec![4, 5]);
    assert_eq!(seqs(&carried_on), vec![4, 5]);
}

/// Seals a segment after about two events.
const SMALL: Retention = Retention {
    segment_bytes: 300,
    keep: Retention::DEFAULT.keep,
};

#[test]
fn a_log_sealed_into_segments_reads_and_subscribes_as_one_stream() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = Log::open_with(dir.path(), DEVICE, SMALL).unwrap();
    append_n(&mut log, 3);
    let mut subscription = subscribe(dir.path(), 1);
    let first = subscription.poll().unwrap();
    append_n(&mut log, 6);
    let second = subscription.poll().unwrap();
    drop(log);
    let mut log = Log::open_with(dir.path(), DEVICE, SMALL).unwrap();
    let next = append_n(&mut log, 1);

    let sealed = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("events."))
        .filter(|entry| entry.file_name() != FILE)
        .count();
    assert!(sealed >= 3, "sealed segments: {sealed}");
    assert_eq!(next, vec![10], "the numbering carries on across segments");
    assert_eq!(
        read(dir.path())
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>()
    );
    assert_eq!(seqs(&first), vec![2, 3]);
    assert_eq!(seqs(&second), (4..=9).collect::<Vec<_>>());
    assert_eq!(
        seqs(&subscribe(dir.path(), 4).poll().unwrap()),
        (5..=10).collect::<Vec<_>>()
    );
}

/// Writes a sealed segment of events `seqs`, each made `days_ago`.
fn sealed_segment(dir: &Path, seqs: std::ops::RangeInclusive<u64>, days_ago: u64) {
    let then = std::time::SystemTime::now() - std::time::Duration::from_secs(days_ago * 86_400);
    let ms = then
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let first = *seqs.start();
    let lines: String = seqs
        .map(|seq| {
            let event = Event {
                v: VERSION,
                device_id: DEVICE.to_owned(),
                seq,
                ulid: ulid::Ulid::from_parts(ms, u128::from(seq)).to_string(),
                chat: Some("CHAT".to_owned()),
                run: None,
                parent_run: None,
                kind: "chat.opened".to_owned(),
                body: serde_json::json!({}),
            };
            format!("{}\n", serde_json::to_string(&event).unwrap())
        })
        .collect();
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(sealed_name(first)), lines).unwrap();
}

#[test]
fn a_sealed_segment_older_than_the_retention_is_deleted_and_the_newest_one_never_is() {
    let dir = tempfile::tempdir().unwrap();
    sealed_segment(dir.path(), 1..=2, 40);
    sealed_segment(dir.path(), 3..=4, 31);
    sealed_segment(dir.path(), 5..=6, 35);

    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    let next = append_n(&mut log, 1);

    assert_eq!(
        read(dir.path())
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect::<Vec<_>>(),
        vec![5, 6, 7],
        "the newest sealed segment holds the last seq when the one being written is empty"
    );
    assert_eq!(next, vec![7]);
}

#[test]
fn a_sealed_segment_inside_the_retention_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    sealed_segment(dir.path(), 1..=2, 29);
    sealed_segment(dir.path(), 3..=4, 1);

    let _log = Log::open(dir.path(), DEVICE).unwrap();

    assert_eq!(read(dir.path()).unwrap().len(), 4);
}

#[test]
fn a_cursor_older_than_the_log_keeps_is_told_what_it_missed_and_carries_on_from_the_oldest_kept() {
    let dir = tempfile::tempdir().unwrap();
    sealed_segment(dir.path(), 1..=2, 40);
    sealed_segment(dir.path(), 3..=4, 1);
    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    append_n(&mut log, 1);

    let got = subscribe(dir.path(), 1).poll().unwrap();

    assert_eq!(
        got.first(),
        Some(&Delivery::Missed {
            after: 1,
            resumes_at: 3
        })
    );
    assert_eq!(seqs(&got), vec![3, 4, 5]);
    assert_eq!(
        seqs(&subscribe(dir.path(), 2).poll().unwrap()),
        vec![3, 4, 5],
        "a cursor at the edge of what is kept missed nothing"
    );
    assert!(
        !subscribe(dir.path(), 2)
            .poll()
            .unwrap()
            .iter()
            .any(|d| matches!(d, Delivery::Missed { .. }))
    );
}

#[test]
fn a_hole_in_the_middle_of_the_log_is_told_as_missed_never_skipped() {
    let dir = tempfile::tempdir().unwrap();
    // A clock that jumped: the first segment is dated recent, the middle one old.
    sealed_segment(dir.path(), 1..=2, 1);
    sealed_segment(dir.path(), 3..=4, 40);
    sealed_segment(dir.path(), 5..=6, 1);
    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    append_n(&mut log, 1);
    let on_disk: Vec<u64> = read(dir.path()).unwrap().iter().map(|e| e.seq).collect();
    let got = subscribe(dir.path(), 2).poll().unwrap();

    assert_eq!(
        on_disk,
        vec![1, 2, 5, 6, 7],
        "the middle segment was pruned"
    );
    assert_eq!(
        got.first(),
        Some(&Delivery::Missed {
            after: 2,
            resumes_at: 5
        })
    );
    assert_eq!(seqs(&got), vec![5, 6, 7]);
}

#[test]
fn a_reader_on_a_live_segment_sealed_and_pruned_between_polls_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    let tiny = Retention {
        segment_bytes: 300,
        keep: std::time::Duration::from_millis(1),
    };
    let mut log = Log::open_with(dir.path(), DEVICE, tiny).unwrap();
    append_n(&mut log, 1);
    let mut subscription = subscribe(dir.path(), 0);
    let first = seqs(&subscription.poll().unwrap());
    std::thread::sleep(std::time::Duration::from_millis(20));
    append_n(&mut log, 1); // seals segment 1
    std::thread::sleep(std::time::Duration::from_millis(20));
    append_n(&mut log, 5); // seals more, prunes segment 1
    let mut later = Vec::new();
    for _ in 0..5 {
        later.extend(subscription.poll().unwrap());
    }
    let on_disk: Vec<u64> = read(dir.path()).unwrap().iter().map(|e| e.seq).collect();

    assert_eq!(first, vec![1]);
    assert!(
        later.iter().any(|d| matches!(d, Delivery::Missed { .. }))
            || seqs(&later).last() == on_disk.last(),
        "stalled: later {:?}, on disk {on_disk:?}",
        seqs(&later)
    );
}

#[test]
fn a_seal_whose_next_segment_cannot_be_opened_writes_nothing_more_into_the_sealed_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = Log::open_with(dir.path(), DEVICE, SMALL).unwrap();
    // The seal's own open and the next append's retry both fail, as a full directory would.
    log.fail_the_next_opens(2);

    let sealed_by = append_n(&mut log, 2);
    let refused = log.append(
        Some("CHAT"),
        Some("RUN"),
        None,
        "hook.stop",
        serde_json::json!({}),
    );
    let after = append_n(&mut log, 1);

    assert_eq!(sealed_by, vec![1, 2]);
    assert!(refused.is_err(), "no segment is open to write to");
    assert_eq!(after, vec![3], "the refused append used no seq");
    let sealed = std::fs::read_to_string(dir.path().join(sealed_name(1))).unwrap();
    assert_eq!(
        sealed.lines().count(),
        2,
        "the sealed segment is never written again"
    );
    assert_eq!(
        read(dir.path())
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn a_seal_hands_its_hook_the_segment_the_next_events_go_to() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = Log::open_with(dir.path(), DEVICE, SMALL).unwrap();
    let handed = std::sync::Arc::new(std::sync::Mutex::new(Vec::<File>::new()));
    let keep = std::sync::Arc::clone(&handed);
    log.on_seal(move |file| keep.lock().unwrap().push(file.try_clone().unwrap()));

    append_n(&mut log, 3);

    let handed = handed.lock().unwrap();
    assert_eq!(handed.len(), 1, "one seal, after the second event");
    let live = std::fs::metadata(dir.path().join(FILE)).unwrap().len();
    assert!(live > 0);
    assert_eq!(
        handed[0].metadata().unwrap().len(),
        live,
        "the handle the hook got is the file the third event is in"
    );
}

#[cfg(unix)]
#[test]
fn a_drained_line_is_recorded_under_the_chat_and_run_it_ran_in_and_says_it_was_spooled() {
    use crate::hookwire::spool::{Drained, Spooled};
    let dir = tempfile::tempdir().unwrap();
    let plane = Path::new("/plane");
    let mut recorder = recorder(dir.path());
    // The run chat 3 was in when the host went away, as the reopen record still says it.
    recorder.knows(
        plane,
        3,
        RunOf {
            chat: "01J9ZZCHAT00000000000000AA",
            run: "01J9ZZRUN000000000000000AA",
        },
    );

    let call = tool_call(3, "pretooluse", crate::hookwire::Decision::Allow);
    for item in [
        Drained::Line {
            chat: 3,
            seq: 1,
            line: Spooled::Tool(call),
        },
        Drained::Gap {
            chat: 3,
            from: 2,
            to: 4,
        },
        Drained::Rejected {
            chat: 3,
            seq: Some(7),
            why: crate::hookwire::spool::why::MAC,
        },
        Drained::Spool {
            chat: 3,
            from: 1,
            to: 6,
        },
    ] {
        recorder.spooled(plane, item).unwrap();
    }

    let events = read(dir.path()).unwrap();
    let kinds: Vec<&str> = events.iter().map(|event| event.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "hook.pretooluse",
            "hook.spool.gap",
            "hook.spool.rejected",
            "hook.spool.drained"
        ],
        "no run.started: a drained line is the run it ran in"
    );
    for event in &events {
        assert_eq!(event.chat.as_deref(), Some("01J9ZZCHAT00000000000000AA"));
        assert_eq!(event.run.as_deref(), Some("01J9ZZRUN000000000000000AA"));
    }
    assert_eq!(events[0].body["spooled"], 1);
    assert_eq!(events[0].body["decision"], "allow");
    assert_eq!(
        (
            events[1].body["from"].as_u64(),
            events[1].body["to"].as_u64()
        ),
        (Some(2), Some(4))
    );
    assert_eq!(events[2].body["seq"], 7);
    assert_eq!(events[2].body["why"], "mac");
    assert_eq!(events[3].body["to"], 6);
}

#[cfg(unix)]
#[test]
fn a_drained_line_of_a_chat_the_host_cannot_name_is_recorded_under_its_number() {
    use crate::hookwire::spool::{Drained, Spooled};
    let dir = tempfile::tempdir().unwrap();
    let mut recorder = recorder(dir.path());

    recorder
        .spooled(
            Path::new("/plane"),
            Drained::Line {
                chat: 9,
                seq: 1,
                line: Spooled::Report(report(9, crate::state::Event::Stop)),
            },
        )
        .unwrap();

    let events = read(dir.path()).unwrap();
    assert_eq!(events.len(), 1, "no run is made up for it: {events:?}");
    assert_eq!(events[0].kind, "hook.stop");
    assert_eq!(events[0].chat, None);
    assert_eq!(events[0].body["chat_number"], 9);
}

#[test]
fn an_event_is_made_durable_through_its_number() {
    let dir = tempfile::tempdir().unwrap();
    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    let durable = log.durable();
    let event = log
        .append(None, None, None, "hook.stop", serde_json::json!({}))
        .unwrap();

    durable.through(event.seq).unwrap();

    assert_eq!(durable.synced(), event.seq);
    durable.through(event.seq).unwrap();
    assert_eq!(
        durable.synced(),
        event.seq,
        "a number already durable is not synced again"
    );
}

#[cfg(unix)]
#[test]
fn what_makes_events_durable_follows_each_seal_onto_the_segment_being_written() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().unwrap();
    let mut log = Log::open_with(dir.path(), DEVICE, SMALL).unwrap();
    let durable = log.durable();
    let before = append_n(&mut log, 1)[0];
    durable.through(before).unwrap();

    // Enough to seal at least once, and one more event in the new segment.
    let seqs = append_n(&mut log, 8);
    assert!(
        !segments(dir.path()).unwrap().is_empty(),
        "the log sealed a segment"
    );
    let last = *seqs.last().unwrap();
    durable.through(last).unwrap();

    let live = std::fs::metadata(dir.path().join(FILE)).unwrap().ino();
    assert_eq!(
        durable.inode(),
        Some(live),
        "the handle synced is the segment being written, not a sealed one"
    );
    assert_eq!(durable.synced(), last);
}

// -------------------------------------------------------------------------------------
// The sandbox's trust events (ADR 0067 §7, ADR 0075 as amended): until the audit exists,
// they are written here, under the chat and the run they are about.
// -------------------------------------------------------------------------------------

#[test]
fn a_person_turning_the_sandbox_off_is_a_trust_event_under_the_chats_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = recorder(dir.path());

    let event = host
        .trust(
            RunOf {
                chat: CHAT,
                run: RUN,
            },
            &crate::sandbox::Change::Off(crate::sandbox::Lifted {
                by: crate::sandbox::By::Person,
                reason: Some("needs the network".to_owned()),
            }),
            Some(crate::harness::Harness::ClaudeCode),
            Some("steward"),
        )
        .unwrap();

    assert_eq!(event.kind, "trust.sandbox.off");
    assert_eq!(
        (event.chat.as_deref(), event.run.as_deref()),
        (Some(CHAT), Some(RUN))
    );
    assert_eq!(event.device_id, DEVICE, "the machine it happened on");
    assert_eq!(
        event.body,
        serde_json::json!({
            "actor_kind": "human",
            "actor": "operator",
            "scope": "local-ui",
            "harness": "claude",
            "persona": "steward",
            "reason": "needs the network",
            "lifted": ["vaults", "integrity", "human-powers", "runner-internals", "later-code"],
        })
    );
    assert_eq!(read(dir.path()).unwrap().last(), Some(&event), "written");
}

#[test]
fn a_windows_start_without_the_sandbox_names_charter_as_the_actor_never_the_operator() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = recorder(dir.path());

    let event = host
        .trust(
            RunOf {
                chat: CHAT,
                run: RUN,
            },
            &crate::sandbox::Change::Off(crate::sandbox::Lifted {
                by: crate::sandbox::By::NoBackend(crate::sandbox::Os::Windows),
                reason: None,
            }),
            Some(crate::harness::Harness::Codex),
            None,
        )
        .unwrap();

    assert_eq!(
        event.body,
        serde_json::json!({
            "actor_kind": "host",
            "actor": "charter (no backend on this OS)",
            "os": "windows",
            "harness": "codex",
            "persona": null,
            "reason": null,
            "lifted": ["vaults", "integrity", "human-powers", "runner-internals", "later-code"],
        })
    );
}

#[test]
fn the_sandbox_coming_back_on_for_a_chat_is_a_trust_event_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = recorder(dir.path());

    let event = host
        .trust(
            RunOf {
                chat: CHAT,
                run: RUN,
            },
            &crate::sandbox::Change::On,
            Some(crate::harness::Harness::ClaudeCode),
            None,
        )
        .unwrap();

    assert_eq!(event.kind, "trust.sandbox.on");
    assert_eq!(
        event.body,
        serde_json::json!({
            "actor_kind": "host",
            "actor": "charter",
            "harness": "claude",
            "persona": null,
            "restored": ["vaults", "integrity", "human-powers", "runner-internals", "later-code"],
        })
    );
}
