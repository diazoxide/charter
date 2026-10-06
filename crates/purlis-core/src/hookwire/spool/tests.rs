use super::*;
use crate::hookwire::{ChatTokens, Decision, ToolCall};

fn call(chat: u32, id: &str) -> ToolCall {
    ToolCall {
        chat,
        tool_hook: "pretooluse".to_owned(),
        tool: Some("Bash".to_owned()),
        call: Some(id.to_owned()),
        args: None,
        decision: Decision::Allow,
        rule: None,
        hook_ms: 1,
        agent: None,
        at_ms: 0,
    }
}

/// A host's spool directory with chat `chat` issued a token, as a listener issues one.
fn issued(dir: &Path, chat: u32) -> crate::hookwire::ChatToken {
    ChatTokens::spooling_into(dir.to_path_buf())
        .issue_to_this_process(chat)
        .expect("a token")
}

fn drained(dir: &Path) -> Vec<Drained> {
    let mut all = Vec::new();
    drain(dir, &mut |item| {
        all.push(item);
        Ok(())
    })
    .expect("the spool drains");
    all
}

fn tools(items: &[Drained]) -> Vec<(u32, u64, String)> {
    items
        .iter()
        .filter_map(|item| match item {
            Drained::Line {
                chat,
                seq,
                line: Spooled::Tool(call),
            } => Some((*chat, *seq, call.call.clone().unwrap_or_default())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_spooled_line_is_drained_once_in_its_chats_order_with_its_number() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 3);

    for id in ["a", "b", "c"] {
        append(&spool, 3, &token, &call(3, id)).expect("the line is spooled");
    }

    let first = drained(&spool);
    assert_eq!(
        tools(&first),
        [
            (3, 1, "a".to_owned()),
            (3, 2, "b".to_owned()),
            (3, 3, "c".to_owned())
        ]
    );
    assert!(
        first.contains(&Drained::Spool {
            chat: 3,
            from: 1,
            to: 3
        }),
        "{first:?}"
    );
    assert_eq!(drained(&spool), [], "a drained line is never drained again");
}

#[test]
fn a_line_missing_from_the_middle_of_a_spool_is_recorded_as_a_gap() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    for id in ["a", "b", "c", "d"] {
        append(&spool, 4, &token, &call(4, id)).expect("the line is spooled");
    }
    // Something that can write the file takes out the second and third lines.
    let file = spool.join("4.jsonl");
    let text = std::fs::read_to_string(&file).expect("the spool");
    let lines: Vec<&str> = text.lines().collect();
    std::fs::write(&file, format!("{}\n{}\n", lines[0], lines[3])).expect("the edit");

    let items = drained(&spool);

    assert_eq!(
        tools(&items),
        [(4, 1, "a".to_owned()), (4, 4, "d".to_owned())]
    );
    assert!(
        items.contains(&Drained::Gap {
            chat: 4,
            from: 2,
            to: 3
        }),
        "{items:?}"
    );
}

#[test]
fn a_line_written_into_another_chats_spool_is_rejected_and_never_read_as_that_chat() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let tokens = ChatTokens::spooling_into(spool.clone());
    let mine = tokens.issue_to_this_process(5).expect("a token");
    let _theirs = tokens.issue_to_this_process(6).expect("a token");

    // Chat 5 holds its own token only. Whatever it writes into chat 6's spool, under its own
    // key or none, names chat 6 and is not chat 6's.
    append(&spool, 6, &mine, &call(6, "forged")).expect("written");
    let forged_line = std::fs::read_to_string(spool.join("6.jsonl")).expect("the spool");
    let unkeyed = forged_line.replace("\"mac\":\"", "\"mac\":\"00");
    std::fs::write(spool.join("6.jsonl"), format!("{forged_line}{unkeyed}")).expect("written");

    let items = drained(&spool);

    assert_eq!(tools(&items), [], "{items:?}");
    let rejected = items
        .iter()
        .filter(|item| matches!(item, Drained::Rejected { chat: 6, .. }))
        .count();
    assert_eq!(rejected, 2, "{items:?}");
}

#[test]
fn a_line_the_drain_cannot_hand_on_is_left_in_the_spool() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 8);
    append(&spool, 8, &token, &call(8, "a")).expect("the line is spooled");

    let failed = drain(&spool, &mut |_| Err(std::io::Error::other("a full disk")));

    assert!(failed.is_err());
    assert_eq!(
        tools(&drained(&spool)),
        [(8, 1, "a".to_owned())],
        "an event the host could not record is drained at the next start"
    );
}

#[test]
fn a_spool_line_holds_no_token() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 2);
    append(&spool, 2, &token, &call(2, "a")).expect("spooled");

    let text = std::fs::read_to_string(spool.join("2.jsonl")).expect("the spool");
    let keys = std::fs::read_to_string(spool.join(KEYS)).expect("the keys");
    assert!(!text.contains(token.expose()), "{text}");
    assert!(!keys.contains(token.expose()), "{keys}");
}

#[test]
fn the_spool_and_its_keys_are_private_to_their_owner() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 1);
    append(&spool, 1, &token, &call(1, "a")).expect("spooled");

    for path in [spool.join("1.jsonl"), spool.join(KEYS)] {
        let mode = std::fs::metadata(&path)
            .expect("there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{}", path.display());
    }
}

#[test]
fn nothing_is_spooled_and_no_key_written_where_the_sandbox_does_not_deny_the_directory() {
    let dir = tempfile::tempdir().expect("a directory");
    // A fallback hook channel's directory, outside any project's `.charter/app/`.
    let elsewhere = dir.path().join("charter-op-0123456789abcdef").join(DIR);
    let token = ChatTokens::default()
        .issue_to_this_process(1)
        .expect("a token");

    let remembered = remember(&elsewhere, 1, &token);
    let spooled = append(&elsewhere, 1, &token, &call(1, "a"));

    assert!(remembered.is_err(), "a key was written outside the denial");
    assert!(spooled.is_err(), "a line was spooled outside the denial");
    assert!(!elsewhere.join(KEYS).exists());
    assert!(!elsewhere.join("1.jsonl").exists());
}

#[test]
fn a_line_that_is_not_text_is_rejected_and_neither_the_drain_nor_the_next_hook_stops_on_it() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    // A write torn through a character, then nothing after it.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(spool.join("4.jsonl"))
        .expect("the spool");
    std::io::Write::write_all(&mut file, b"{\"v\":1,\"line\":\"\xe2\x80").expect("written");
    drop(file);
    append(&spool, 4, &token, &call(4, "b")).expect("the next hook still spools");
    let other = issued(&spool, 5);
    append(&spool, 5, &other, &call(5, "c")).expect("spooled");

    let items = drained(&spool);

    assert_eq!(
        tools(&items),
        [
            (4, 1, "a".to_owned()),
            (4, 2, "b".to_owned()),
            (5, 1, "c".to_owned())
        ]
    );
    assert!(
        items.contains(&Drained::Rejected {
            chat: 4,
            seq: None,
            why: why::UNREADABLE
        }),
        "{items:?}"
    );
}

#[test]
fn a_keys_file_that_does_not_read_rejects_its_lines_as_no_key_and_the_drain_finishes() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 6);
    append(&spool, 6, &token, &call(6, "a")).expect("spooled");
    std::fs::write(spool.join(KEYS), b"{not json").expect("written");

    let items = drained(&spool);

    assert_eq!(
        items,
        [Drained::Rejected {
            chat: 6,
            seq: Some(1),
            why: why::NO_KEY
        }]
    );
    assert_eq!(drained(&spool), [], "drained, not stuck");
    issued(&spool, 7);
}

#[test]
fn a_spool_another_process_holds_is_given_up_on_in_bounded_time_and_the_line_is_lost() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("the first line is spooled");
    // A second open of the file is a second lock holder to flock, as another process is.
    let holder = File::open(file_for(&spool, 4)).expect("the spool opens");
    holder.lock().expect("the holder takes the lock");

    let started = std::time::Instant::now();
    let refused = append(&spool, 4, &token, &call(4, "b")).expect_err("the line is not spooled");
    let waited = started.elapsed();

    assert_eq!(refused.kind(), io::ErrorKind::TimedOut, "{refused}");
    assert!(
        waited >= A_LOCK_IS_WAITED_FOR_AT_MOST && waited < Duration::from_secs(2),
        "waited {waited:?}"
    );
    drop(holder);
    append(&spool, 4, &token, &call(4, "c")).expect("a free spool takes the next line");
    assert_eq!(
        tools(&drained(&spool)),
        [(4, 1, "a".to_owned()), (4, 2, "c".to_owned())],
        "the line given up on is not in the spool, and nothing is numbered for it"
    );
}

/// What `append` answers for chat `chat` in `spool`, or `None` if it has not answered in two
/// seconds: a hook must answer its harness whatever sits at the spool path.
fn appended_within_two_seconds(
    spool: &Path,
    chat: u32,
    token: crate::hookwire::ChatToken,
) -> Option<io::Result<u64>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let spool = spool.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(append(&spool, chat, &token, &call(chat, "a")));
    });
    rx.recv_timeout(Duration::from_secs(2)).ok()
}

#[test]
fn a_spool_path_that_is_not_a_plain_file_is_refused_at_once_and_the_line_is_lost() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 5);
    // A named pipe, where nothing ever writes: a blocking open or read of it never returns.
    let made =
        crate::forklock::status(std::process::Command::new("mkfifo").arg(file_for(&spool, 5)))
            .expect("mkfifo runs");
    assert!(made.success(), "mkfifo");

    let answered = appended_within_two_seconds(&spool, 5, token).expect("the hook answered");
    let refused = answered.expect_err("nothing is spooled into a pipe");
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{refused}");
}

#[test]
fn a_spool_that_is_not_a_plain_file_is_reported_and_the_drain_reads_the_others() {
    /// What sits at the spool path, and how it is put there.
    type Planted<'a> = (&'a str, &'a dyn Fn(&Path));
    let plant: [Planted<'_>; 4] = [
        ("a pipe", &|at: &Path| {
            let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(at))
                .expect("mkfifo runs");
            assert!(made.success(), "mkfifo");
        }),
        ("a directory", &|at: &Path| {
            std::fs::create_dir(at).expect("mkdir")
        }),
        ("a dangling link", &|at: &Path| {
            std::os::unix::fs::symlink(at.with_extension("gone"), at).expect("a link");
        }),
        ("a live link", &|at: &Path| {
            let to = at.with_extension("real");
            std::fs::write(&to, b"").expect("its target");
            std::os::unix::fs::symlink(&to, at).expect("a link");
        }),
    ];
    for (what, plant) in plant {
        let dir = tempfile::tempdir().expect("a directory");
        let spool = dir.path().join(".charter/app").join(DIR);
        let token = issued(&spool, 7);
        issued(&spool, 5);
        append(&spool, 7, &token, &call(7, "a")).expect("the line is spooled");
        plant(&file_for(&spool, 5));

        let all = drained(&spool);
        assert!(
            all.contains(&Drained::Rejected {
                chat: 5,
                seq: None,
                why: why::UNREADABLE
            }),
            "{what}: {all:?}"
        );
        assert_eq!(tools(&all), [(7, 1, "a".to_owned())], "{what}");
    }
}
