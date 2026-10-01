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
            "hook.stop",
            serde_json::json!({}),
        )
        .unwrap();
    let second = log
        .append(
            Some("CHAT"),
            Some("RUN"),
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
            log.append(Some("CHAT"), None, "chat.opened", serde_json::json!({}))
                .unwrap();
        }
    }

    let mut log = Log::open(dir.path(), DEVICE).unwrap();
    let next = log
        .append(Some("CHAT"), None, "chat.opened", serde_json::json!({}))
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
    host.report(
        plane,
        &report(7, SessionStart),
        Followed::From("conversation-a"),
    )
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
        log.append(Some("CHAT"), None, "chat.opened", serde_json::json!({}))
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
        .append(Some("CHAT"), None, "chat.opened", serde_json::json!({}))
        .unwrap();

    let events = read(dir.path()).unwrap();
    assert_eq!(events.len(), 2, "the torn line went, the new one is whole: {events:#?}");
    assert_eq!(events[0].seq, 1);
    assert_eq!(next.seq, 2);
    assert_eq!(events[1], next);
}

#[test]
fn a_log_with_lines_but_no_number_in_them_is_refused_rather_than_counted_from_one_again() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE), "not an event\nnor this\n").unwrap();

    let refused = Log::open(dir.path(), DEVICE);

    assert!(refused.is_err(), "a seq is never reused, so a log it cannot read is not restarted");
}

#[test]
fn a_last_seq_at_the_top_of_the_range_is_refused_rather_than_wrapped() {
    let dir = tempfile::tempdir().unwrap();
    let event = Event {
        v: VERSION,
        device_id: DEVICE.to_owned(),
        seq: u64::MAX,
        ulid: ulid::Ulid::new().to_string(),
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
