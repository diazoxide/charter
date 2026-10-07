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

/// The names in chat `chat`'s spool folder, sorted.
fn names(spool: &Path, chat: u32) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder_for(spool, chat))
        .expect("the chat's folder")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .into_string()
                .expect("a name")
        })
        .collect();
    names.sort();
    names
}

/// The file key `token`'s line number `seq` is in, in chat `chat`'s spool.
fn line_file(spool: &Path, chat: u32, token: &crate::hookwire::ChatToken, seq: u64) -> PathBuf {
    folder_for(spool, chat).join(name_of(&SpoolKey::of(token).id(), seq, Kind::Line))
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
    assert_eq!(names(&spool, 3), Vec::<String>::new(), "its file is gone");
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
    // Something that can write the folder takes out the second and third lines.
    for seq in [2, 3] {
        std::fs::remove_file(line_file(&spool, 4, &token, seq)).expect("the line's file");
    }

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
    let forged_line = std::fs::read_to_string(line_file(&spool, 6, &mine, 1)).expect("the line");
    let unkeyed = forged_line.replace("\"mac\":\"", "\"mac\":\"00");
    std::fs::write(line_file(&spool, 6, &mine, 2), unkeyed).expect("written");

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

    let text = std::fs::read_to_string(line_file(&spool, 2, &token, 1)).expect("the line");
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

    for (path, private) in [
        (line_file(&spool, 1, &token, 1), 0o600),
        (spool.join(KEYS), 0o600),
        (folder_for(&spool, 1), 0o700),
    ] {
        let mode = std::fs::metadata(&path)
            .expect("there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, private, "{}", path.display());
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
    assert!(!folder_for(&elsewhere, 1).exists());
}

#[test]
fn a_file_that_is_not_a_line_is_rejected_and_neither_the_drain_nor_the_next_hook_stops_on_it() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    // A line torn through a character under the next number, an empty one, and a file no hook
    // names.
    std::fs::write(
        line_file(&spool, 4, &token, 2),
        b"{\"v\":1,\"line\":\"\xe2\x80",
    )
    .expect("written");
    std::fs::write(line_file(&spool, 4, &token, 3), b"").expect("written");
    std::fs::write(folder_for(&spool, 4).join("notes.txt"), b"hello").expect("written");
    append(&spool, 4, &token, &call(4, "b")).expect("the next hook still spools");
    let other = issued(&spool, 5);
    append(&spool, 5, &other, &call(5, "c")).expect("spooled");

    let items = drained(&spool);

    assert_eq!(
        tools(&items),
        [
            (4, 1, "a".to_owned()),
            (4, 4, "b".to_owned()),
            (5, 1, "c".to_owned())
        ]
    );
    let unreadable = Drained::Rejected {
        chat: 4,
        seq: None,
        why: why::UNREADABLE,
    };
    assert_eq!(
        items.iter().filter(|item| **item == unreadable).count(),
        3,
        "{items:?}"
    );
    assert!(
        items.contains(&Drained::Gap {
            chat: 4,
            from: 2,
            to: 3
        }),
        "{items:?}"
    );
    assert_eq!(names(&spool, 4), Vec::<String>::new(), "all of it is gone");
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
fn a_hook_spools_while_another_process_holds_the_chats_spool() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("the first line is spooled");
    // The lock a drain holds on the folder while it reads and hands on, for as long as it
    // takes. A second open is a second holder to flock, as another process is.
    let holder = File::open(folder_for(&spool, 4)).expect("the folder opens");
    holder.lock().expect("the holder takes the lock");

    let spooled = append(&spool, 4, &token, &call(4, "b")).expect("no hook waits for it");

    assert_eq!(spooled, 2);
    drop(holder);
    assert_eq!(
        tools(&drained(&spool)),
        [(4, 1, "a".to_owned()), (4, 2, "b".to_owned())]
    );
}

#[test]
fn a_disk_that_does_not_take_the_line_in_time_is_given_up_on_and_the_line_is_said_lost() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    let a_disk_that_hangs = Bounds {
        wait: Duration::from_millis(100),
        a_line_reaches_the_disk: || {
            std::thread::sleep(Duration::from_secs(1));
            Ok(())
        },
        ..Bounds::A_HOOKS
    };

    let started = std::time::Instant::now();
    let refused = append_within(&spool, 4, &token, &call(4, "a"), a_disk_that_hangs)
        .expect_err("the hook does not wait for it");
    let waited = started.elapsed();

    assert_eq!(refused.kind(), io::ErrorKind::TimedOut, "{refused}");
    assert!(refused.to_string().contains("within 100 ms"), "{refused}");
    assert!(
        waited >= Duration::from_millis(100) && waited < Duration::from_millis(900),
        "waited {waited:?}"
    );
}

#[test]
fn a_disk_that_refuses_the_line_says_so_and_leaves_nothing_behind() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    let a_full_disk = Bounds {
        a_line_reaches_the_disk: || Err(io::Error::other("no space left on the device")),
        ..Bounds::A_HOOKS
    };

    let refused = append_within(&spool, 4, &token, &call(4, "a"), a_full_disk)
        .expect_err("the line is not spooled");

    assert!(refused.to_string().contains("no space left"), "{refused}");
    assert_eq!(
        names(&spool, 4),
        Vec::<String>::new(),
        "no number is held for a line that was never kept"
    );
    append(&spool, 4, &token, &call(4, "b")).expect("a disk with room takes the next line");
    assert_eq!(tools(&drained(&spool)), [(4, 1, "b".to_owned())]);
}

#[test]
fn a_spool_past_what_a_hook_lists_takes_no_more_and_the_line_is_said_lost() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    let three_lines = Bounds {
        lines: 3,
        ..Bounds::A_HOOKS
    };
    for id in ["a", "b", "c"] {
        append_within(&spool, 4, &token, &call(4, id), three_lines).expect("there is room");
    }

    let refused = append_within(&spool, 4, &token, &call(4, "d"), three_lines)
        .expect_err("the spool is full");

    assert_eq!(refused.kind(), io::ErrorKind::InvalidData, "{refused}");
    assert!(
        refused.to_string().contains("already holds 3 lines"),
        "{refused}"
    );
    assert_eq!(names(&spool, 4).len(), 3, "nothing was written for it");
    assert_eq!(tools(&drained(&spool)).len(), 3);
}

#[test]
fn a_number_a_dead_hook_took_is_a_gap_and_its_file_goes_once_it_is_old() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    // A hook that took the second number and died before its line was whole.
    let part = folder_for(&spool, 4).join(name_of(&SpoolKey::of(&token).id(), 2, Kind::Part));
    std::fs::write(&part, b"{\"v\":1,\"se").expect("written");
    let third = append(&spool, 4, &token, &call(4, "c")).expect("the next hook still spools");

    let items = drained(&spool);

    assert_eq!(third, 3, "the dead hook's number is not given twice");
    assert_eq!(
        tools(&items),
        [(4, 1, "a".to_owned()), (4, 3, "c".to_owned())]
    );
    assert!(
        items.contains(&Drained::Gap {
            chat: 4,
            from: 2,
            to: 2
        }),
        "{items:?}"
    );
    let unfinished = Drained::Rejected {
        chat: 4,
        seq: Some(2),
        why: why::UNFINISHED,
    };
    assert!(items.contains(&unfinished), "{items:?}");
    assert!(part.exists(), "a hook may still be writing it");

    // Written long ago, or at a time that has not come: either way no hook is writing it.
    let now = std::time::SystemTime::now();
    for when in [
        now - 2 * A_PART_IS_A_DEAD_HOOKS_AFTER,
        now + 2 * A_PART_IS_A_DEAD_HOOKS_AFTER,
    ] {
        std::fs::write(&part, b"").expect("written");
        File::options()
            .write(true)
            .open(&part)
            .and_then(|file| file.set_modified(when))
            .expect("its time is set");
        assert_eq!(
            drained(&spool),
            std::slice::from_ref(&unfinished),
            "said, as it is removed"
        );
        assert!(!part.exists(), "no hook lives that long");
    }
}

#[test]
fn a_part_file_under_the_last_number_there_is_stops_its_keys_hooks_and_the_drain_says_so() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    let part =
        folder_for(&spool, 4).join(name_of(&SpoolKey::of(&token).id(), u64::MAX, Kind::Part));
    std::fs::write(&part, b"").expect("planted");

    let refused = append(&spool, 4, &token, &call(4, "b")).expect_err("no number is left");

    assert!(
        refused.to_string().contains("used every number"),
        "{refused}"
    );
    let items = drained(&spool);
    assert_eq!(tools(&items), [(4, 1, "a".to_owned())]);
    assert!(
        items.contains(&Drained::Rejected {
            chat: 4,
            seq: Some(u64::MAX),
            why: why::UNFINISHED,
        }),
        "{items:?}"
    );
}

/// What the drain does with a line that was still being written while it ran (issue 983's
/// first follow-up): its number is a gap at that drain, and the host forgets the key when the
/// drain ends, so the line is rejected at the next one and its content is never recorded.
#[test]
fn a_line_in_flight_while_a_drain_runs_is_a_gap_then_and_no_key_at_the_next() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    // A hook has taken the second number and is still writing its line.
    let part = folder_for(&spool, 4).join(name_of(&SpoolKey::of(&token).id(), 2, Kind::Part));
    std::fs::write(&part, b"").expect("its number is taken");
    append(&spool, 4, &token, &call(4, "c")).expect("spooled");

    let first = drained(&spool);
    // The hook finishes after the drain has.
    std::fs::write(&part, a_spool_line(4, &token, 2, "b")).expect("written");
    std::fs::rename(&part, line_file(&spool, 4, &token, 2)).expect("named");
    let second = drained(&spool);

    assert_eq!(
        tools(&first),
        [(4, 1, "a".to_owned()), (4, 3, "c".to_owned())]
    );
    for said in [
        Drained::Gap {
            chat: 4,
            from: 2,
            to: 2,
        },
        Drained::Rejected {
            chat: 4,
            seq: Some(2),
            why: why::UNFINISHED,
        },
    ] {
        assert!(first.contains(&said), "{said:?} is not in {first:?}");
    }
    assert_eq!(
        second,
        [Drained::Rejected {
            chat: 4,
            seq: Some(2),
            why: why::NO_KEY,
        }]
    );
}

/// The same line when the host still holds its key at the next drain, which it does only when
/// the drain it was in flight for stopped before its end: handed on then, after a gap for the
/// numbers the stopped drain had already taken.
#[test]
fn a_line_in_flight_while_a_drain_that_stops_runs_is_handed_on_at_the_next() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    let other = issued(&spool, 8);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    let part = folder_for(&spool, 4).join(name_of(&SpoolKey::of(&token).id(), 2, Kind::Part));
    std::fs::write(&part, b"").expect("its number is taken");
    append(&spool, 8, &other, &call(8, "x")).expect("spooled");

    // The host records chat 4's line and cannot record chat 8's.
    let stopped = drain(&spool, &mut |item| match item.chat() {
        8 => Err(io::Error::other("a full disk")),
        _ => Ok(()),
    });
    std::fs::write(&part, a_spool_line(4, &token, 2, "b")).expect("written");
    std::fs::rename(&part, line_file(&spool, 4, &token, 2)).expect("named");
    let next = drained(&spool);

    assert!(stopped.is_err());
    assert_eq!(
        tools(&next),
        [(4, 2, "b".to_owned()), (8, 1, "x".to_owned())]
    );
    assert!(
        next.contains(&Drained::Gap {
            chat: 4,
            from: 1,
            to: 1
        }),
        "{next:?}"
    );
}

#[test]
fn a_line_copied_from_the_folder_into_an_older_builds_file_is_handed_on_once() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    let mut copied = Vec::new();
    for (seq, id) in [(1, "a"), (2, "b")] {
        append(&spool, 4, &token, &call(4, id)).expect("spooled");
        copied.extend(std::fs::read(line_file(&spool, 4, &token, seq)).expect("the line"));
    }
    // Something that can write the spool copies both lines, MAC and all, into the file a
    // build before #983 wrote. It needs no token for that.
    std::fs::write(file_for(&spool, 4), copied).expect("the copy");

    let items = drained(&spool);

    assert_eq!(
        tools(&items),
        [(4, 1, "a".to_owned()), (4, 2, "b".to_owned())],
        "{items:?}"
    );
    for seq in [1, 2] {
        let repeated = Drained::Rejected {
            chat: 4,
            seq: Some(seq),
            why: why::REPEATED,
        };
        assert!(
            items.contains(&repeated),
            "{repeated:?} is not in {items:?}"
        );
    }
}

#[test]
fn a_line_put_back_after_its_drain_is_rejected_as_no_key() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    let saved = std::fs::read(line_file(&spool, 4, &token, 1)).expect("the line");
    assert_eq!(tools(&drained(&spool)), [(4, 1, "a".to_owned())]);

    std::fs::write(line_file(&spool, 4, &token, 1), saved).expect("put back");

    assert_eq!(
        drained(&spool),
        [Drained::Rejected {
            chat: 4,
            seq: Some(1),
            why: why::NO_KEY,
        }]
    );
}

#[test]
fn a_line_file_is_one_line_under_its_own_key_and_number_or_it_is_rejected() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    for id in ["a", "b", "c"] {
        append(&spool, 4, &token, &call(4, id)).expect("spooled");
    }
    // The second line under a number that is not its own, the third under a key that is not
    // its own, and two lines that check in one file.
    std::fs::rename(
        line_file(&spool, 4, &token, 2),
        line_file(&spool, 4, &token, 9),
    )
    .expect("renamed");
    std::fs::rename(
        line_file(&spool, 4, &token, 3),
        folder_for(&spool, 4).join("0000000000000000.3.json"),
    )
    .expect("renamed");
    std::fs::write(
        line_file(&spool, 4, &token, 4),
        a_spool_line(4, &token, 4, "d") + &a_spool_line(4, &token, 5, "e"),
    )
    .expect("written");

    let items = drained(&spool);

    assert_eq!(tools(&items), [(4, 1, "a".to_owned())], "{items:?}");
    let unreadable = Drained::Rejected {
        chat: 4,
        seq: None,
        why: why::UNREADABLE,
    };
    assert_eq!(
        items.iter().filter(|item| **item == unreadable).count(),
        3,
        "{items:?}"
    );
    assert_eq!(names(&spool, 4), Vec::<String>::new(), "all of it is gone");
}

#[test]
fn a_link_a_directory_or_a_pipe_under_a_lines_name_is_rejected_and_never_followed() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    append(&spool, 4, &token, &call(4, "a")).expect("spooled");
    // A link to a line that would check, outside the folder; a directory; a pipe nobody
    // writes to.
    let outside = dir.path().join("outside.json");
    std::fs::write(&outside, a_spool_line(4, &token, 2, "linked")).expect("its target");
    std::os::unix::fs::symlink(&outside, line_file(&spool, 4, &token, 2)).expect("a link");
    std::fs::create_dir(line_file(&spool, 4, &token, 3)).expect("a directory");
    let made = crate::forklock::status(
        std::process::Command::new("mkfifo").arg(line_file(&spool, 4, &token, 4)),
    )
    .expect("mkfifo runs");
    assert!(made.success(), "mkfifo");
    let next = append(&spool, 4, &token, &call(4, "b")).expect("the next hook still spools");

    let items = drained(&spool);

    assert_eq!(
        next, 5,
        "a name that is taken is stepped past, never written through"
    );
    assert_eq!(
        tools(&items),
        [(4, 1, "a".to_owned()), (4, 5, "b".to_owned())]
    );
    let unreadable = Drained::Rejected {
        chat: 4,
        seq: None,
        why: why::UNREADABLE,
    };
    assert_eq!(
        items.iter().filter(|item| **item == unreadable).count(),
        3,
        "{items:?}"
    );
    assert!(outside.exists(), "the link's target is not touched");
    assert_eq!(
        names(&spool, 4),
        [name_of(&SpoolKey::of(&token).id(), 3, Kind::Line)],
        "the link and the pipe are gone; a directory is left for its owner"
    );
}

#[test]
fn the_cap_counts_every_name_in_the_folder_whatever_it_is() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    let three_names = Bounds {
        lines: 3,
        ..Bounds::A_HOOKS
    };
    append_within(&spool, 4, &token, &call(4, "a"), three_names).expect("there is room");
    // A number another key's hook holds, and a file no hook names.
    std::fs::write(folder_for(&spool, 4).join("0000000000000000.7.part"), b"").expect("written");
    std::fs::write(folder_for(&spool, 4).join("notes.txt"), b"").expect("written");

    let refused = append_within(&spool, 4, &token, &call(4, "b"), three_names)
        .expect_err("the spool is full");

    assert_eq!(refused.kind(), io::ErrorKind::InvalidData, "{refused}");
    assert_eq!(names(&spool, 4).len(), 3, "nothing was written for it");
}

#[test]
fn a_write_that_stops_without_answering_says_so_and_not_that_the_disk_was_slow() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    let a_write_that_panics = Bounds {
        a_line_reaches_the_disk: || panic!("this test's own, to stop the write"),
        ..Bounds::A_HOOKS
    };

    let refused = append_within(&spool, 4, &token, &call(4, "a"), a_write_that_panics)
        .expect_err("the line is not spooled");

    assert_ne!(refused.kind(), io::ErrorKind::TimedOut, "{refused}");
    assert!(refused.to_string().contains("stopped"), "{refused}");
}

/// The object a spool line is, with its newline: what a line's file holds, and what a build
/// before #983 appended to the chat's one file.
fn a_spool_line(chat: u32, token: &crate::hookwire::ChatToken, seq: u64, id: &str) -> String {
    let key = SpoolKey::of(token);
    let line = serde_json::to_string(&call(chat, id)).expect("a line");
    let mac = key.sign(chat, seq, &key.id(), &line);
    let mut text = serde_json::to_string(&OnDisk {
        v: VERSION,
        seq,
        key: key.id(),
        line,
        mac,
    })
    .expect("a spool line");
    text.push('\n');
    text
}

#[test]
fn a_spool_file_an_older_build_left_is_drained_before_the_chats_folder_and_emptied() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 4);
    let other = issued(&spool, 6);
    // Chat 4 has both: an older hook's file, with a line a crash tore, and this build's lines.
    let old = file_for(&spool, 4);
    let mut text = a_spool_line(4, &token, 1, "old-a");
    text.push_str(&a_spool_line(4, &token, 3, "old-c"));
    text.push_str("{\"v\":1,\"seq\":4,\"ke");
    std::fs::write(&old, text).expect("the old spool");
    append(&spool, 4, &token, &call(4, "new-a")).expect("spooled");
    // Chat 6 has the old file alone.
    std::fs::write(file_for(&spool, 6), a_spool_line(6, &other, 1, "old-only"))
        .expect("the old spool");

    let items = drained(&spool);

    assert_eq!(
        tools(&items),
        [
            (4, 1, "old-a".to_owned()),
            (4, 3, "old-c".to_owned()),
            (4, 1, "new-a".to_owned()),
            (6, 1, "old-only".to_owned())
        ]
    );
    for found in [
        Drained::Gap {
            chat: 4,
            from: 2,
            to: 2,
        },
        Drained::Rejected {
            chat: 4,
            seq: None,
            why: why::UNREADABLE,
        },
        Drained::Spool {
            chat: 4,
            from: 1,
            to: 3,
        },
        Drained::Spool {
            chat: 4,
            from: 1,
            to: 1,
        },
    ] {
        assert!(items.contains(&found), "{found:?} is not in {items:?}");
    }
    assert_eq!(
        std::fs::read(&old).expect("still there, for a hook holding it open"),
        b"",
        "emptied"
    );
    assert!(
        !folder_for(&spool, 6).exists(),
        "nothing is made to read it"
    );
    assert_eq!(drained(&spool), [], "drained once");
}

#[test]
fn two_keys_lines_are_handed_on_in_the_order_the_host_issued_the_keys() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let now = std::time::SystemTime::now();
    let mut started = Vec::new();
    // Chat 4 started three times, each start with a token of its own, and spooled under each.
    // The files' times say the opposite order, and are not what is read.
    for (start, ago) in [(0, 10), (1, 20), (2, 30)] {
        let token = issued(&spool, 4);
        for line in 1..=2 {
            let id = format!("start-{start}-{line}");
            let seq = append(&spool, 4, &token, &call(4, &id)).expect("spooled");
            File::options()
                .write(true)
                .open(line_file(&spool, 4, &token, seq))
                .and_then(|file| file.set_modified(now - Duration::from_secs(ago)))
                .expect("its time is set");
            started.push((4, seq, id));
        }
    }

    assert_eq!(tools(&drained(&spool)), started);
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
fn a_spool_path_that_is_not_a_folder_is_refused_at_once_and_the_line_is_lost() {
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 5);
    // A named pipe, where nothing ever writes: a blocking open or read of it never returns.
    let made =
        crate::forklock::status(std::process::Command::new("mkfifo").arg(folder_for(&spool, 5)))
            .expect("mkfifo runs");
    assert!(made.success(), "mkfifo");

    let answered = appended_within_two_seconds(&spool, 5, token).expect("the hook answered");
    let refused = answered.expect_err("nothing is spooled into a pipe");
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{refused}");
}

#[test]
fn a_spool_that_is_not_what_it_should_be_is_reported_and_the_drain_reads_the_others() {
    /// What sits at the spool path, and how it is put there.
    type Planted<'a> = (&'a str, &'a dyn Fn(&Path));
    let pipe: Planted<'_> = ("a pipe", &|at: &Path| {
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(at))
            .expect("mkfifo runs");
        assert!(made.success(), "mkfifo");
    });
    let directory: Planted<'_> = ("a directory", &|at: &Path| {
        std::fs::create_dir(at).expect("mkdir")
    });
    let file: Planted<'_> = ("a file", &|at: &Path| {
        std::fs::write(at, b"").expect("a file")
    });
    let dangling: Planted<'_> = ("a dangling link", &|at: &Path| {
        std::os::unix::fs::symlink(at.with_extension("gone"), at).expect("a link");
    });
    let to_a_file: Planted<'_> = ("a link to a file", &|at: &Path| {
        let to = at.with_extension("real");
        std::fs::write(&to, b"").expect("its target");
        std::os::unix::fs::symlink(&to, at).expect("a link");
    });
    let to_a_folder: Planted<'_> = ("a link to a folder", &|at: &Path| {
        let to = at.with_extension("real");
        std::fs::create_dir(&to).expect("its target");
        std::os::unix::fs::symlink(&to, at).expect("a link");
    });
    /// Where chat 5's spool is: this build's folder, or an older build's file.
    type Spool = fn(&Path, u32) -> PathBuf;
    let planted: [(Spool, &[Planted<'_>]); 2] = [
        (file_for, &[pipe, directory, dangling, to_a_file]),
        (folder_for, &[pipe, file, dangling, to_a_file, to_a_folder]),
    ];
    for (at, plants) in planted {
        for (what, plant) in plants {
            let dir = tempfile::tempdir().expect("a directory");
            let spool = dir.path().join(".charter/app").join(DIR);
            let token = issued(&spool, 7);
            issued(&spool, 5);
            append(&spool, 7, &token, &call(7, "a")).expect("the line is spooled");
            let at = at(&spool, 5);
            plant(&at);

            let all = drained(&spool);
            assert!(
                all.contains(&Drained::Rejected {
                    chat: 5,
                    seq: None,
                    why: why::UNREADABLE
                }),
                "{what} at {}: {all:?}",
                at.display()
            );
            assert_eq!(tools(&all), [(7, 1, "a".to_owned())], "{what}");
        }
    }
}

/// A hook's bounds on a disk that takes 20 ms to make a line durable, as a loaded machine's does.
fn on_a_slow_disk() -> Bounds {
    Bounds {
        a_line_reaches_the_disk: || {
            std::thread::sleep(Duration::from_millis(20));
            Ok(())
        },
        ..Bounds::A_HOOKS
    }
}

/// Issue 983: hooks of one chat that spool at once never cost each other a line. Sixteen
/// writers, two thousand lines, and a disk slow enough that a writer queued behind the others'
/// syncs would run out of any wait a hook can afford.
#[test]
fn sixteen_hooks_spooling_two_thousand_lines_for_one_chat_on_a_slow_disk_lose_none() {
    const WRITERS: usize = 16;
    const EACH: usize = 125;
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 9);
    let lost = std::sync::Mutex::new(Vec::new());

    std::thread::scope(|scope| {
        for writer in 0..WRITERS {
            let (spool, token, lost) = (&spool, &token, &lost);
            scope.spawn(move || {
                for n in 0..EACH {
                    let id = format!("{writer:02}-{n:03}");
                    if let Err(why) =
                        append_within(spool, 9, token, &call(9, &id), on_a_slow_disk())
                    {
                        lost.lock().unwrap().push(format!("{id}: {why}"));
                    }
                }
            });
        }
    });

    let lost = lost.into_inner().unwrap();
    assert!(
        lost.is_empty(),
        "{} of {} lines were not spooled, the first of them: {:#?}",
        lost.len(),
        WRITERS * EACH,
        &lost[..lost.len().min(5)]
    );
    let items = drained(&spool);
    let lines = tools(&items);
    assert_eq!(
        lines.iter().map(|(_, seq, _)| *seq).collect::<Vec<_>>(),
        (1..=(WRITERS * EACH) as u64).collect::<Vec<_>>(),
        "every line has a number of its own, and none is missing"
    );
    assert_eq!(
        items.len(),
        WRITERS * EACH + 1,
        "nothing but the lines and their sequence: no gap, nothing rejected"
    );
    // Each writer's lines are in the order it spooled them.
    for writer in 0..WRITERS {
        let mine: Vec<&String> = lines
            .iter()
            .map(|(_, _, id)| id)
            .filter(|id| id.starts_with(&format!("{writer:02}-")))
            .collect();
        let mut sorted = mine.clone();
        sorted.sort();
        assert_eq!(mine.len(), EACH, "writer {writer}");
        assert_eq!(mine, sorted, "writer {writer}'s lines are out of its order");
    }
}

/// A drain that runs while hooks spool, as a host's one drain at a project's open would if
/// hooks of the last run were still about. It hands on each line it read, once, and removes
/// those files and no other. Every line a hook finishes after that is still in the spool, and
/// the next drain rejects it as `no-key`, because the first drain forgot the key (issue 983's
/// first follow-up). A number the first drain called a gap is one of those lines.
#[test]
fn a_drain_racing_hooks_hands_on_what_it_read_once_and_the_rest_is_no_key_at_the_next() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    const WRITERS: usize = 8;
    const EACH: usize = 100;
    let dir = tempfile::tempdir().expect("a directory");
    let spool = dir.path().join(".charter/app").join(DIR);
    let token = issued(&spool, 9);
    let spooled = AtomicUsize::new(0);

    let racing = std::thread::scope(|scope| {
        for writer in 0..WRITERS {
            let (spool, token, spooled) = (&spool, &token, &spooled);
            scope.spawn(move || {
                for n in 0..EACH {
                    let id = format!("{writer:02}-{n:03}");
                    append(spool, 9, token, &call(9, &id)).expect("the line is spooled");
                    spooled.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
        // The drain starts once the hooks are well under way.
        while spooled.load(Ordering::SeqCst) < WRITERS * EACH / 4 {
            std::thread::yield_now();
        }
        drained(&spool)
    });
    let after = drained(&spool);

    let handed_on = tools(&racing);
    let mut once: Vec<&String> = handed_on.iter().map(|(_, _, id)| id).collect();
    once.sort();
    once.dedup();
    assert_eq!(once.len(), handed_on.len(), "a line was handed on twice");
    assert!(!handed_on.is_empty(), "the drain read nothing");
    let mut no_key = Vec::new();
    for item in &after {
        match item {
            Drained::Rejected {
                seq: Some(seq),
                why: why::NO_KEY,
                ..
            } => no_key.push(*seq),
            other => panic!("the next drain found {other:?}"),
        }
    }
    assert_eq!(
        handed_on.len() + no_key.len(),
        WRITERS * EACH,
        "each line was handed on by the first drain or rejected by the next, and none is gone"
    );
    for item in &racing {
        if let Drained::Gap { from, to, .. } = item {
            for missing in *from..=*to {
                assert!(
                    no_key.contains(&missing),
                    "the gap at {missing} is not a line that was in flight: {item:?}"
                );
            }
        }
    }
    assert_eq!(names(&spool, 9), Vec::<String>::new(), "nothing is left");
}
