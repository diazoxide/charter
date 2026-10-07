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

/// Drains `spool`, or `None` if the drain has not finished in five seconds: a process holding
/// one chat's spool must not hold up the app's start.
fn drained_within_five_seconds(spool: &Path) -> Option<Vec<Drained>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let spool = spool.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(drained(&spool));
    });
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

#[test]
fn a_spool_another_process_holds_is_reported_held_and_drained_at_the_next_start() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let held = issued(&spool, 4);
    let free = issued(&spool, 5);
    append(&spool, 4, &held, &call(4, "a")).expect("spooled");
    append(&spool, 5, &free, &call(5, "b")).expect("spooled");
    let holder = File::open(file_for(&spool, 4)).expect("the spool opens");
    holder.lock().expect("the holder takes the lock");

    let all = drained_within_five_seconds(&spool).expect("the drain finished");

    assert!(
        all.contains(&Drained::Rejected {
            chat: 4,
            seq: None,
            why: why::HELD
        }),
        "{all:?}"
    );
    assert_eq!(tools(&all), [(5, 1, "b".to_owned())]);
    drop(holder);
    assert_eq!(
        tools(&drained(&spool)),
        [(4, 1, "a".to_owned())],
        "the held spool, and the key it checks under, are kept for the next start"
    );
}

#[test]
fn a_spool_past_what_the_drain_reads_is_reported_and_kept() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let big = issued(&spool, 4);
    let other = issued(&spool, 5);
    append(&spool, 4, &big, &call(4, "a")).expect("spooled");
    append(&spool, 5, &other, &call(5, "b")).expect("spooled");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(file_for(&spool, 4))
        .expect("the spool");
    file.set_len(A_SPOOL_HOLDS_AT_MOST + 1).expect("grown");
    drop(file);

    let all = drained(&spool);

    assert!(
        all.contains(&Drained::Rejected {
            chat: 4,
            seq: None,
            why: why::TOO_BIG
        }),
        "{all:?}"
    );
    assert_eq!(tools(&all), [(5, 1, "b".to_owned())]);
    let len = std::fs::metadata(file_for(&spool, 4)).expect("kept").len();
    assert_eq!(len, A_SPOOL_HOLDS_AT_MOST + 1, "left as it was");
}

#[test]
fn a_line_that_would_take_a_spool_past_what_the_drain_reads_is_refused_unread() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(file_for(&spool, 4))
        .expect("the spool");
    file.set_len(A_SPOOL_HOLDS_AT_MOST - 8).expect("grown");
    drop(file);

    let refused = append(&spool, 4, &token, &call(4, "b")).expect_err("not spooled");

    assert_eq!(refused.kind(), io::ErrorKind::InvalidData, "{refused}");
}

/// A reader that counts what is read from it.
struct Counted<'a> {
    inner: std::io::Cursor<&'a [u8]>,
    read: u64,
}

impl Read for Counted<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        Ok(n)
    }
}

impl std::io::Seek for Counted<'_> {
    fn seek(&mut self, to: std::io::SeekFrom) -> io::Result<u64> {
        self.inner.seek(to)
    }
}

fn on_disk(seq: u64, key: &str) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&OnDisk {
            v: VERSION,
            seq,
            key: key.to_owned(),
            line: "{}".to_owned(),
            mac: String::new(),
        })
        .expect("a line")
    )
}

#[test]
fn the_next_number_is_read_off_the_spools_tail() {
    // A long downtime: thousands of this key's lines, the last one numbered 5000.
    let mut text: String = (1..=5000).map(|seq| on_disk(seq, "k")).collect();
    let mut file = Counted {
        inner: std::io::Cursor::new(text.as_bytes()),
        read: 0,
    };
    let len = text.len() as u64;
    assert_eq!(
        last_number(&mut file, len, "k").expect("read"),
        (5000, true)
    );
    assert!(
        file.read <= 2 * A_TAIL_IS_READ_BY,
        "read {} of {len} bytes",
        file.read
    );

    // Another key's lines after it, and a line torn at the end, are read past.
    text.push_str(&on_disk(1, "other"));
    text.push_str("{\"v\":1,\"seq\":9");
    let len = text.len() as u64;
    let mut file = Counted {
        inner: std::io::Cursor::new(text.as_bytes()),
        read: 0,
    };
    assert_eq!(
        last_number(&mut file, len, "k").expect("read"),
        (5000, false)
    );

    // A line longer than one read of the tail is read whole.
    let long = format!("{}{}", on_disk(7, "k").trim_end(), " ".repeat(200_000));
    let text = format!("{long}\n{}", on_disk(1, "other"));
    let mut file = Counted {
        inner: std::io::Cursor::new(text.as_bytes()),
        read: 0,
    };
    assert_eq!(
        last_number(&mut file, text.len() as u64, "k").expect("read"),
        (7, true)
    );
    // And a spool with none of this key's lines starts it at 1.
    let mut file = Counted {
        inner: std::io::Cursor::new(b"" as &[u8]),
        read: 0,
    };
    assert_eq!(last_number(&mut file, 0, "k").expect("read"), (0, true));
}

#[test]
fn an_empty_spool_has_its_directory_synced_again_by_the_next_append() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    // What a directory sync that timed out leaves: the file made, and nothing in it.
    std::fs::write(file_for(&spool, 4), b"").expect("an empty spool");

    let before = DIRECTORY_SYNCS.with(std::cell::Cell::get);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    let synced = DIRECTORY_SYNCS.with(std::cell::Cell::get) - before;

    assert_eq!(synced, 1, "the directory was not synced");
    append(&spool, 4, &token, &call(4, "b")).expect("spooled");
    assert_eq!(
        DIRECTORY_SYNCS.with(std::cell::Cell::get) - before,
        1,
        "a spool with lines in it is not synced again"
    );
}

#[test]
fn a_sync_still_running_past_its_wait_is_given_up_on() {
    let started = Instant::now();
    let given_up = done_within(Duration::from_millis(50), "the line", || {
        std::thread::sleep(Duration::from_secs(2));
        Ok(())
    })
    .expect_err("given up on");
    assert_eq!(given_up.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(done_within(Duration::from_secs(1), "the line", || Ok(())).is_ok());
}

#[test]
fn a_line_not_yet_durable_is_told_apart_from_a_line_lost() {
    let late = not_yet_durable(io::Error::new(io::ErrorKind::TimedOut, "slow"));
    assert_eq!(late.kind(), io::ErrorKind::TimedOut);
    assert!(is_not_yet_durable(&late), "{late}");
    assert!(late.to_string().contains("next start"), "{late}");
    assert!(!is_not_yet_durable(&io::Error::other("lost")));
}

/// What `remember` answers for chat `chat` in `spool`, or `None` if it has not answered in five
/// seconds: issuing a token must not wait on a process holding the spool directory.
fn remembered_within_five_seconds(spool: &Path, chat: u32) -> Option<io::Result<()>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let spool = spool.to_path_buf();
    std::thread::spawn(move || {
        let token = ChatTokens::default()
            .issue_to_this_process(chat)
            .expect("a token");
        let _ = tx.send(remember(&spool, chat, &token));
    });
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

/// A second open of the spool directory, holding its lock: another process reading or writing
/// `keys.json`, as flock sees one (#1426).
fn hold_the_keys(spool: &Path) -> File {
    let holder = File::open(spool).expect("the directory opens");
    holder.lock().expect("the holder takes the lock");
    holder
}

#[test]
fn a_token_whose_keys_another_process_holds_is_given_up_on_in_bounded_time() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    issued(&spool, 3);
    let holder = hold_the_keys(&spool);

    let started = Instant::now();
    let refused = remembered_within_five_seconds(&spool, 4)
        .expect("remember answered")
        .expect_err("the key is not recorded");
    let waited = started.elapsed();

    assert_eq!(refused.kind(), io::ErrorKind::TimedOut, "{refused}");
    assert!(
        waited >= THE_KEYS_ARE_WAITED_FOR_AT_MOST && waited < Duration::from_secs(3),
        "waited {waited:?}"
    );
    assert!(refused.to_string().contains("keys"), "{refused}");
    drop(holder);
    remembered_within_five_seconds(&spool, 4)
        .expect("remember answered")
        .expect("free keys take the key");
}

#[test]
fn a_drain_whose_keys_another_process_holds_stops_in_bounded_time_and_keeps_every_line() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    let holder = hold_the_keys(&spool);

    let (tx, rx) = std::sync::mpsc::channel();
    let at = spool.clone();
    let started = Instant::now();
    std::thread::spawn(move || {
        let _ = tx.send(drain(&at, &mut |_| Ok(())));
    });
    let refused = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the drain answered")
        .expect_err("the drain stops");
    let waited = started.elapsed();

    assert_eq!(refused.kind(), io::ErrorKind::TimedOut, "{refused}");
    assert!(
        waited >= THE_KEYS_ARE_WAITED_FOR_AT_MOST && waited < Duration::from_secs(3),
        "waited {waited:?}"
    );
    drop(holder);
    assert_eq!(
        tools(&drained(&spool)),
        [(4, 1, "a".to_owned())],
        "nothing was read, so the line and its key are there for the next start"
    );
}

#[test]
fn a_drains_last_sync_still_running_past_its_wait_is_given_up_on_and_the_drain_finishes() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    let other = issued(&spool, 5);
    append(&spool, 5, &other, &call(5, "b")).expect("spooled");

    let (tx, rx) = std::sync::mpsc::channel();
    let at = spool.clone();
    let started = Instant::now();
    std::thread::spawn(move || {
        THE_DRAINS_SYNC_STALLS_FOR.with(|stall| stall.set(Duration::from_secs(5)));
        let _ = tx.send(drained(&at));
    });
    let all = rx
        .recv_timeout(Duration::from_secs(8))
        .expect("the drain answered");
    let took = started.elapsed();

    assert_eq!(
        tools(&all),
        [(4, 1, "a".to_owned()), (5, 1, "b".to_owned())]
    );
    assert!(
        // Two waits and room for a loaded machine, still short of the stall.
        took < 2 * THE_DRAINS_SYNC_IS_WAITED_FOR_AT_MOST + Duration::from_secs(2),
        "took {took:?}"
    );
    assert_eq!(
        tools(&drained(&spool)),
        [],
        "each spool was emptied all the same"
    );
}
