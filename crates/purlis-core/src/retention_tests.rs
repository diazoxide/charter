use super::*;
use std::path::PathBuf;

/// A plane directory, resolved the way the app resolves one.
fn plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

/// A file at `path`, last written `age` ago.
fn aged(path: &Path, age: Duration) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"x\n").unwrap();
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - age).unwrap();
}

const OLD: Duration = Duration::from_secs(31 * 24 * 60 * 60);
const YOUNG: Duration = Duration::from_secs(29 * 24 * 60 * 60);

#[test]
fn a_session_marker_untouched_for_thirty_days_is_collected_and_a_younger_one_is_kept() {
    let (_d, root) = plane();
    let sessions = root.join(".charter/sessions");
    aged(&sessions.join("0cb42edd-a97f.memnudge"), OLD);
    aged(&sessions.join("0cb42edd-a97f.configver"), OLD);
    aged(&sessions.join("7.workspace"), YOUNG);

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.sessions, 2);
    assert!(!sessions.join("0cb42edd-a97f.memnudge").exists());
    assert!(!sessions.join("0cb42edd-a97f.configver").exists());
    assert!(sessions.join("7.workspace").exists());
}

#[test]
fn a_live_chat_keeps_every_marker_it_has_however_old() {
    let (_d, root) = plane();
    let sessions = root.join(".charter/sessions");
    aged(&sessions.join("3.workspace"), OLD);
    aged(&sessions.join("3.lock"), OLD);
    aged(&sessions.join("3.toolu_01.routing-ask.ask-pending"), OLD);
    // Chat 30 is not chat 3.
    aged(&sessions.join("30.workspace"), OLD);
    // A live chat's conversation keys the usage ring.
    aged(&sessions.join("9f1c-uuid.usage"), OLD);

    let swept = sweep(&root, SystemTime::now(), &["3".into(), "9f1c-uuid".into()]);

    assert_eq!(swept.sessions, 1);
    assert!(!sessions.join("30.workspace").exists());
    for kept in [
        "3.workspace",
        "3.lock",
        "3.toolu_01.routing-ask.ask-pending",
        "9f1c-uuid.usage",
    ] {
        assert!(sessions.join(kept).exists(), "{kept} was removed");
    }
}

#[test]
fn the_tool_ceilings_and_names_charter_never_makes_are_not_this_sweeps() {
    let (_d, root) = plane();
    let sessions = root.join(".charter/sessions");
    // `personagate::sweep_ceilings` owns these, and removes them in an order.
    aged(&sessions.join("old.tools"), OLD);
    aged(&sessions.join("old.gate"), OLD);
    // Not a name charter writes.
    aged(&sessions.join("a b.workspace"), OLD);
    aged(&sessions.join(".hidden"), OLD);
    std::fs::create_dir_all(sessions.join("dir.workspace")).unwrap();

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.sessions, 0);
    for kept in [
        "old.tools",
        "old.gate",
        "a b.workspace",
        ".hidden",
        "dir.workspace",
    ] {
        assert!(sessions.join(kept).exists(), "{kept} was removed");
    }
}

#[test]
fn a_session_trace_untouched_for_thirty_days_is_collected_unless_its_session_is_live() {
    let (_d, root) = plane();
    let trace = crate::trace::file(&root, "x");
    let trace = trace.parent().unwrap();
    aged(&trace.join("ended.jsonl"), OLD);
    aged(&trace.join("4.jsonl"), OLD);
    aged(&trace.join("recent.jsonl"), YOUNG);
    aged(&trace.join("notes.txt"), OLD);

    let swept = sweep(&root, SystemTime::now(), &["4".into()]);

    assert_eq!(swept.traces, 1);
    assert!(!trace.join("ended.jsonl").exists());
    for kept in ["4.jsonl", "recent.jsonl", "notes.txt"] {
        assert!(trace.join(kept).exists(), "{kept} was removed");
    }
}

#[test]
fn a_report_draft_untouched_for_thirty_days_is_collected_and_nothing_else_there_is() {
    let (_d, root) = plane();
    let reports = root.join(".charter/reports");
    aged(&reports.join("0ae5a4c2dee96f44.json"), OLD);
    aged(&reports.join("1b4880c22cd68294.json"), YOUNG);
    // Not a fingerprint: the Python charter's ids are 16 lowercase hex digits.
    aged(&reports.join("notes.json"), OLD);
    aged(&reports.join("0ae5a4c2dee96f4.json"), OLD);

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.reports, 1);
    assert!(!reports.join("0ae5a4c2dee96f44.json").exists());
    for kept in [
        "1b4880c22cd68294.json",
        "notes.json",
        "0ae5a4c2dee96f4.json",
    ] {
        assert!(reports.join(kept).exists(), "{kept} was removed");
    }
}

#[cfg(unix)]
#[test]
fn a_store_reached_through_a_link_is_not_swept() {
    let (_d, root) = plane();
    let elsewhere = tempfile::tempdir().unwrap();
    aged(&elsewhere.path().join("theirs.memnudge"), OLD);
    aged(&elsewhere.path().join("0ae5a4c2dee96f44.json"), OLD);
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.join(".charter/sessions")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.join(".charter/reports")).unwrap();

    assert_eq!(sweep(&root, SystemTime::now(), &[]), Swept::default());

    assert!(elsewhere.path().join("theirs.memnudge").exists());
    assert!(elsewhere.path().join("0ae5a4c2dee96f44.json").exists());
}

#[cfg(unix)]
#[test]
fn a_marker_that_is_a_link_is_not_followed() {
    let (_d, root) = plane();
    let elsewhere = tempfile::tempdir().unwrap();
    let theirs = elsewhere.path().join("keep.txt");
    aged(&theirs, OLD);
    let sessions = root.join(".charter/sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::os::unix::fs::symlink(&theirs, sessions.join("x.workspace")).unwrap();

    assert_eq!(sweep(&root, SystemTime::now(), &[]).sessions, 0);

    assert!(theirs.exists());
}

#[test]
fn the_hook_spool_and_the_rest_of_the_state_directory_are_never_touched() {
    let (_d, root) = plane();
    let state = root.join(".charter");
    let others = [
        "app/spool/1.memnudge",
        "app/spool/0ae5a4c2dee96f44.json",
        "app/reopen.json",
        "eventlog/0001.jsonl",
        "terminals/-9.workspace",
        "save-journal.jsonl",
        "chat-turns/3",
    ];
    for other in others {
        aged(&state.join(other), OLD);
    }

    assert_eq!(sweep(&root, SystemTime::now(), &[]), Swept::default());

    for other in others {
        assert!(state.join(other).exists(), "{other} was removed");
    }
}

#[test]
fn opening_a_plane_keeps_what_every_chat_it_will_reopen_is_keyed_on() {
    let (_d, root) = plane();
    let record = crate::reopen::Record {
        chats: vec![crate::reopen::Chat {
            program: "claude".into(),
            number: Some(3),
            resume: Some(crate::harness::SessionId::new("9f1c-uuid").unwrap()),
            ..Default::default()
        }],
        dealt: 3,
        ..Default::default()
    };
    crate::reopen::write(&root, &record).unwrap();
    let sessions = root.join(".charter/sessions");
    let trace = root.join(".charter/persona-state/trace");
    aged(&sessions.join("3.workspace"), OLD);
    aged(&sessions.join("9f1c-uuid.usage"), OLD);
    aged(&trace.join("3.jsonl"), OLD);
    aged(&sessions.join("2.workspace"), OLD);

    let swept = on_open(&root, SystemTime::now());

    assert_eq!((swept.sessions, swept.traces), (1, 0));
    assert!(!sessions.join("2.workspace").exists());
    assert!(sessions.join("3.workspace").exists());
    assert!(sessions.join("9f1c-uuid.usage").exists());
    assert!(trace.join("3.jsonl").exists());
}

#[test]
fn a_reopen_record_that_cannot_be_read_leaves_every_session_file_alone() {
    let (_d, root) = plane();
    // Unreadable, so nothing says which chats come back.
    std::fs::create_dir_all(crate::reopen::path(&root)).unwrap();
    let sessions = root.join(".charter/sessions");
    let trace = root.join(".charter/persona-state/trace");
    aged(&sessions.join("3.workspace"), OLD);
    aged(&trace.join("3.jsonl"), OLD);
    aged(&root.join(".charter/reports/0ae5a4c2dee96f44.json"), OLD);

    let swept = on_open(&root, SystemTime::now());

    assert_eq!(
        swept,
        Swept {
            sessions: 0,
            traces: 0,
            reports: 1,
            dispatches: 0,
            spend: 0,
            gates: 0,
            nudges: 0,
            saved: 0,
            ephemeral: 0,
        }
    );
    assert!(sessions.join("3.workspace").exists());
    assert!(trace.join("3.jsonl").exists());
}

#[test]
fn a_reopen_record_older_than_chat_numbers_leaves_every_session_file_alone() {
    let (_d, root) = plane();
    // Written before a chat kept its number: the next launch deals them again, in order, so
    // nothing here says which pointer is whose.
    let record = crate::reopen::Record {
        chats: vec![crate::reopen::Chat {
            program: "claude".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    crate::reopen::write(&root, &record).unwrap();
    let sessions = root.join(".charter/sessions");
    aged(&sessions.join("1.workspace"), OLD);

    let swept = on_open(&root, SystemTime::now());

    assert_eq!(swept.sessions, 0);
    assert!(sessions.join("1.workspace").exists());
}

/// A reopen record holding `text`, and one month-old file of chat 3 in each session store.
fn a_record_saying(root: &Path, text: &str) {
    let record = crate::reopen::path(root);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, text).unwrap();
    aged(&root.join(".charter/sessions/3.workspace"), OLD);
    aged(&root.join(".charter/persona-state/trace/3.jsonl"), OLD);
}

#[test]
fn a_garbled_reopen_record_leaves_every_session_file_alone() {
    let (_d, root) = plane();
    a_record_saying(&root, "{not json");

    let swept = on_open(&root, SystemTime::now());

    assert_eq!((swept.sessions, swept.traces), (0, 0));
    assert!(root.join(".charter/sessions/3.workspace").exists());
    assert!(root.join(".charter/persona-state/trace/3.jsonl").exists());
}

#[test]
fn a_reopen_record_of_another_version_leaves_every_session_file_alone() {
    let (_d, root) = plane();
    a_record_saying(&root, r#"{"version": 999}"#);

    let swept = on_open(&root, SystemTime::now());

    assert_eq!((swept.sessions, swept.traces), (0, 0));
    assert!(root.join(".charter/sessions/3.workspace").exists());
    assert!(root.join(".charter/persona-state/trace/3.jsonl").exists());
}

#[test]
fn a_plane_with_no_reopen_record_is_swept() {
    let (_d, root) = plane();
    aged(&root.join(".charter/sessions/3.workspace"), OLD);

    assert_eq!(on_open(&root, SystemTime::now()).sessions, 1);
}

/// A month-old trace of `session` holding one `event`, written the way charter writes one.
fn traced(root: &Path, session: &str, event: &str) -> PathBuf {
    let stamp = chrono::NaiveDate::from_ymd_opt(2026, 8, 1)
        .unwrap()
        .and_hms_opt(9, 0, 0)
        .unwrap();
    crate::trace::record_values(
        root,
        session,
        event,
        &[("names", serde_json::json!(["DB_PASSWORD"]))],
        stamp,
    );
    let file = crate::trace::file(root, session);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(SystemTime::now() - OLD)
        .unwrap();
    file
}

#[test]
fn a_trace_that_records_a_secret_handed_out_is_kept_however_old() {
    let (_d, root) = plane();
    let mut kept = Vec::new();
    for (n, event) in crate::secrets::cmd::HANDED_OUT.iter().enumerate() {
        kept.push(traced(&root, &format!("s{n}"), event));
    }
    let gone = traced(&root, "plain", "persona-use");

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.traces, 1);
    assert!(!gone.exists());
    for file in kept {
        assert!(file.exists(), "{} was removed", file.display());
    }
}

#[test]
fn every_event_a_secret_hand_out_is_traced_under_is_one_the_sweep_keeps() {
    // `trace_secret_use` is the one writer of these events; each of its callers names its
    // event as a literal, and every one of them must be in the list the sweep reads.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut named = Vec::new();
    for file in [
        "crates/purlis-core/src/secrets/cmd.rs",
        "crates/purlis-core/src/secrets/exec.rs",
        "crates/purlis-core/src/secrets/identity.rs",
        "app/src-tauri/src/vaults.rs",
    ] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        // A call written on one line or over several: its event is the first string literal
        // after the open parenthesis. The function's own definition names none.
        for (at, found) in text.match_indices("trace_secret_use(") {
            if text[..at].ends_with("fn ") {
                continue;
            }
            let call = &text[at + found.len()..];
            let call = &call[..call.find(')').unwrap_or(call.len())];
            let Some(open) = call.find('"') else {
                continue;
            };
            let event = &call[open + 1..];
            named.push(event[..event.find('"').unwrap_or(event.len())].to_string());
        }
    }
    assert!(named.len() >= 5, "found only {named:?}");
    for event in &named {
        assert!(
            crate::secrets::cmd::HANDED_OUT.contains(&event.as_str()),
            "{event} is traced but the sweep would collect it"
        );
    }
}

// ----- dispatch records (#1452) -----

/// A chat as a dispatch record names it: its number, and its ULID where it has one.
type Named = (u32, Option<&'static str>);

/// A dispatch record from chat `asker` to chat `worker`, last written `age` ago; its path.
fn a_dispatch_record(root: &Path, asker: Named, worker: Named, age: Duration) -> PathBuf {
    use crate::dispatchrecord::{Asker, ChatRef, Mode, Opening, Place, Worker};
    let chat = |(chat, id): Named| ChatRef {
        chat,
        id: id.map(str::to_owned),
        name: format!("chat {chat}"),
        persona: None,
    };
    let record = crate::dispatchrecord::open(
        root,
        Opening {
            mode: Mode::Handoff,
            asker: Asker {
                chat: chat(asker),
                ..Asker::default()
            },
            persona: None,
            worker: Worker {
                chat: chat(worker),
                ..Worker::default()
            },
            task: None,
            place: Place::default(),
            brief: "a brief".into(),
            report_owed: false,
        },
        chrono::Utc::now(),
    )
    .unwrap();
    let path = crate::dispatchrecord::dir(root).join(format!("{}.json", record.id));
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() - age).unwrap();
    path
}

/// A chat the reopen record brings back.
fn live(number: u32, id: Option<&str>) -> crate::dispatchrecord::Live {
    crate::dispatchrecord::Live {
        id: id.map(str::to_owned),
        number: Some(number),
    }
}

const SEPTEMBERS_ASKER: &str = "01K4SEPTEMBERASKER00000000";
const SEPTEMBERS_WORKER: &str = "01K4SEPTEMBERW0RKER0000000";
const OCTOBERS_CHAT: &str = "01K60CT0BERCHAT00000000000";

#[test]
fn a_dispatch_record_untouched_for_thirty_days_is_collected_and_a_younger_one_is_kept() {
    let (_d, root) = plane();
    let old = a_dispatch_record(&root, (1, None), (2, None), OLD);
    let young = a_dispatch_record(&root, (1, None), (2, None), YOUNG);
    // Not a record's name: never this sweep's.
    let other = crate::dispatchrecord::dir(&root).join("notes.json");
    aged(&other, OLD);

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.dispatches, 1);
    assert!(!old.exists());
    assert!(young.exists());
    assert!(other.exists());
}

#[test]
fn a_dispatch_record_is_kept_while_the_chat_that_asked_or_the_chat_that_worked_comes_back() {
    let (_d, root) = plane();
    let asked_by_live = a_dispatch_record(
        &root,
        (3, Some(OCTOBERS_CHAT)),
        (40, Some(SEPTEMBERS_WORKER)),
        OLD,
    );
    let worked_by_live = a_dispatch_record(
        &root,
        (41, Some(SEPTEMBERS_ASKER)),
        (3, Some(OCTOBERS_CHAT)),
        OLD,
    );
    let neither = a_dispatch_record(
        &root,
        (30, Some(SEPTEMBERS_ASKER)),
        (13, Some(SEPTEMBERS_WORKER)),
        OLD,
    );

    // The chat comes back under another number: it was started again in between.
    let swept = sweep_keeping(
        &root,
        SystemTime::now(),
        &[],
        &[live(12, Some(OCTOBERS_CHAT))],
    );

    assert_eq!(swept.dispatches, 1);
    assert!(asked_by_live.exists());
    assert!(worked_by_live.exists());
    assert!(!neither.exists());
}

/// A chat's number is dealt again in another launch. September's handoff from chat 2 to chat 3
/// is not October's chats 2 and 3, and their coming back keeps nothing of it.
#[test]
fn a_chat_that_only_shares_a_number_with_one_long_closed_keeps_none_of_its_records() {
    let (_d, root) = plane();
    let septembers = a_dispatch_record(
        &root,
        (2, Some(SEPTEMBERS_ASKER)),
        (3, Some(SEPTEMBERS_WORKER)),
        OLD,
    );

    let swept = sweep_keeping(
        &root,
        SystemTime::now(),
        &["2".into(), "3".into()],
        &[live(2, Some(OCTOBERS_CHAT)), live(3, None)],
    );

    assert_eq!(swept.dispatches, 1);
    assert!(!septembers.exists());
}

/// Only a record that names its chats by nothing but a number is kept by one.
#[test]
fn a_dispatch_record_with_no_chat_id_is_kept_by_its_chat_s_number() {
    let (_d, root) = plane();
    let numbered = a_dispatch_record(&root, (3, None), (40, None), OLD);
    // Chat 30 is not chat 3.
    let another = a_dispatch_record(&root, (30, None), (13, None), OLD);

    let swept = sweep_keeping(
        &root,
        SystemTime::now(),
        &[],
        &[live(3, Some(OCTOBERS_CHAT))],
    );

    assert_eq!(swept.dispatches, 1);
    assert!(numbered.exists());
    assert!(!another.exists());
}

#[test]
fn opening_a_project_keeps_the_dispatch_records_of_the_chats_it_brings_back_by_their_ids() {
    let (_d, root) = plane();
    let record = crate::reopen::Record {
        chats: vec![crate::reopen::Chat {
            program: "claude".into(),
            number: Some(3),
            identity: crate::reopen::Identity {
                id: Some(OCTOBERS_CHAT.to_owned()),
                ..Default::default()
            },
            ..Default::default()
        }],
        dealt: 3,
        ..Default::default()
    };
    crate::reopen::write(&root, &record).unwrap();
    let its_own = a_dispatch_record(
        &root,
        (9, Some(SEPTEMBERS_ASKER)),
        (7, Some(OCTOBERS_CHAT)),
        OLD,
    );
    let a_namesakes = a_dispatch_record(
        &root,
        (9, Some(SEPTEMBERS_ASKER)),
        (3, Some(SEPTEMBERS_WORKER)),
        OLD,
    );

    let swept = on_open(&root, SystemTime::now());

    assert_eq!(swept.dispatches, 1);
    assert!(its_own.exists());
    assert!(!a_namesakes.exists());
}

#[test]
fn a_reopen_record_that_cannot_be_read_leaves_every_dispatch_record_alone() {
    let (_d, root) = plane();
    let old = a_dispatch_record(&root, (1, None), (2, None), OLD);
    a_record_saying(&root, "{ not json");

    assert_eq!(on_open(&root, SystemTime::now()).dispatches, 0);

    assert!(old.exists());
}

#[test]
fn what_a_cut_short_write_of_a_dispatch_record_left_behind_is_collected_with_the_records() {
    // #1457: a write that is interrupted leaves its temporary file in the store, holding a
    // brief, and nothing else ever removes it.
    let (_d, root) = plane();
    let record = a_dispatch_record(&root, (1, None), (2, None), YOUNG);
    let id = record.file_stem().unwrap().to_string_lossy().into_owned();
    let store = crate::dispatchrecord::dir(&root);
    let old = store.join(format!(".purlis-generated.{id}.json.4171.9f3a.tmp"));
    let young = store.join(format!(".purlis-generated.{id}.json.4172.9f3b.tmp"));
    // Not a record's: another store's temporary file is never this sweep's.
    let other = store.join(".purlis-generated.notes.json.4171.9f3a.tmp");
    aged(&old, OLD);
    aged(&young, YOUNG);
    aged(&other, OLD);

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.dispatches, 1);
    assert!(!old.exists());
    assert!(young.exists());
    assert!(other.exists());
    assert!(record.exists());
}

#[test]
fn a_dispatch_record_is_collected_thirty_days_after_it_ended_however_lately_it_was_rewritten() {
    // #1556: forgetting what the two chats said rewrites the record; that is no reason to keep
    // its brief and its report another month.
    let (_d, root) = plane();
    let now = SystemTime::now();
    let ended_at = |age: Duration| chrono::DateTime::<chrono::Utc>::from(now - age);
    let ended = |age: Duration| {
        let path = a_dispatch_record(&root, (1, None), (2, None), Duration::ZERO);
        let id = path.file_stem().unwrap().to_str().unwrap().to_owned();
        crate::dispatchrecord::close(
            &root,
            &id,
            crate::dispatchrecord::Ending::default(),
            ended_at(age),
        )
        .unwrap();
        // Written again today.
        crate::dispatchrecord::kept_open(&root, &id).unwrap();
        path
    };
    let long_ended = ended(OLD);
    let lately_ended = ended(YOUNG);
    // Still running, and written today.
    let running = a_dispatch_record(&root, (1, None), (2, None), Duration::ZERO);
    // An end that stands in the future is not one: aged from when it was last written.
    let ahead = a_dispatch_record(&root, (1, None), (2, None), Duration::ZERO);
    let mut record: crate::dispatchrecord::Record =
        serde_json::from_str(&std::fs::read_to_string(&ahead).unwrap()).unwrap();
    record.ended = Some(crate::dispatch::stamp(
        chrono::Utc::now() + chrono::Duration::days(400),
    ));
    std::fs::write(&ahead, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    let file = std::fs::File::options().write(true).open(&ahead).unwrap();
    file.set_modified(now - OLD).unwrap();
    // Ended long ago, and its asking chat comes back: kept however old.
    let of_a_live_chat = a_dispatch_record(
        &root,
        (3, Some(OCTOBERS_CHAT)),
        (40, Some(SEPTEMBERS_WORKER)),
        Duration::ZERO,
    );
    let id = of_a_live_chat
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    crate::dispatchrecord::close(
        &root,
        &id,
        crate::dispatchrecord::Ending::default(),
        ended_at(OLD),
    )
    .unwrap();

    let swept = sweep_keeping(&root, now, &[], &[live(3, Some(OCTOBERS_CHAT))]);

    assert_eq!(swept.dispatches, 2);
    assert!(!long_ended.exists());
    assert!(!ahead.exists());
    assert!(lately_ended.exists());
    assert!(running.exists());
    assert!(of_a_live_chat.exists());
}

#[test]
fn a_record_that_still_owes_its_report_is_kept_thirty_days_from_when_it_became_owed() {
    // #1556: the asking chat stayed open 35 days after the task ended, with the report unread,
    // then closed; the record is marked as owing it then. The next open must not collect it,
    // or the chat's reopen is never handed the report.
    use crate::dispatchrecord::{Asker, ChatRef, Mode, Opening, Place, Worker};
    let (_d, root) = plane();
    let now = SystemTime::now();
    let chat = |chat: u32, id: &str| ChatRef {
        chat,
        id: Some(id.to_owned()),
        name: format!("chat {chat}"),
        persona: None,
    };
    let record = crate::dispatchrecord::open(
        &root,
        Opening {
            mode: Mode::Task,
            asker: Asker {
                chat: chat(3, OCTOBERS_CHAT),
                ..Asker::default()
            },
            persona: None,
            worker: Worker {
                chat: chat(40, SEPTEMBERS_WORKER),
                ..Worker::default()
            },
            task: Some("task".to_owned()),
            place: Place::default(),
            brief: "a brief".into(),
            report_owed: true,
        },
        chrono::Utc::now(),
    )
    .unwrap();
    let thirty_five_days = Duration::from_secs(35 * 24 * 60 * 60);
    crate::dispatchrecord::close(
        &root,
        &record.id,
        crate::dispatchrecord::Ending {
            report: Some(crate::dispatchrecord::Report {
                outcome: crate::dispatchrecord::Outcome::Done,
                text: "Done.".to_owned(),
                changed: crate::dispatchrecord::Changed::default(),
            }),
            usage: None,
        },
        chrono::DateTime::<chrono::Utc>::from(now - thirty_five_days),
    )
    .unwrap();
    let path = crate::dispatchrecord::dir(&root).join(format!("{}.json", record.id));
    // While the asking chat was open, its record was kept however old.
    assert_eq!(
        sweep_keeping(&root, now, &[], &[live(3, Some(OCTOBERS_CHAT))]).dispatches,
        0
    );
    // It closed with the report unread: the record says the report is owed, from now.
    assert!(crate::dispatchrecord::kept_undelivered(&root, &record.id, None).unwrap());

    let swept = sweep_keeping(&root, now, &[], &[]);

    assert_eq!(swept.dispatches, 0);
    assert!(
        path.exists(),
        "the report is still owed to the chat that asked"
    );
    // And a month after it became owed, it goes.
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(now - OLD).unwrap();
    assert_eq!(sweep_keeping(&root, now, &[], &[]).dispatches, 1);
}

#[test]
fn a_chats_figure_untouched_for_thirty_days_is_collected_unless_the_chat_comes_back() {
    let (_d, root) = plane();
    let spend = crate::usage::spend_dir(&root);
    aged(&spend.join(format!("{SEPTEMBERS_ASKER}.json")), OLD);
    aged(&spend.join(format!("{SEPTEMBERS_WORKER}.json")), OLD);
    aged(&spend.join(format!("{OCTOBERS_CHAT}.json")), YOUNG);

    let swept = sweep_keeping(
        &root,
        SystemTime::now(),
        &[],
        &[live(3, Some(SEPTEMBERS_ASKER))],
    );

    assert_eq!(swept.spend, 1);
    assert!(spend.join(format!("{SEPTEMBERS_ASKER}.json")).exists());
    assert!(!spend.join(format!("{SEPTEMBERS_WORKER}.json")).exists());
    assert!(spend.join(format!("{OCTOBERS_CHAT}.json")).exists());
}

/// A month-old trace at `name` in the trace store, holding `bytes`.
fn a_trace_holding(root: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(".charter/persona-state/trace").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::now() - OLD)
        .unwrap();
    path
}

/// #1027 (V71): a trace the sweep cannot read whole as text might record a secret handed out,
/// so it is kept, however old — one that is not UTF-8, one past the largest read, and one
/// that cannot be opened.
#[test]
fn a_trace_that_cannot_be_read_whole_as_text_is_kept() {
    let (_d, root) = plane();
    let gone = a_trace_holding(&root, "plain.jsonl", b"{\"event\":\"persona-use\"}\n");
    let not_text = a_trace_holding(&root, "bytes.jsonl", b"{\"event\":\"\xff\xfe\"}\n");
    let large = a_trace_holding(&root, "large.jsonl", b"");
    std::fs::File::options()
        .write(true)
        .open(&large)
        .unwrap()
        .set_len(MOST_TRACE_BYTES + 1)
        .unwrap();
    std::fs::File::options()
        .write(true)
        .open(&large)
        .unwrap()
        .set_modified(SystemTime::now() - OLD)
        .unwrap();
    #[cfg(unix)]
    let unreadable = {
        use std::os::unix::fs::PermissionsExt;
        let path = a_trace_holding(&root, "shut.jsonl", b"{\"event\":\"persona-use\"}\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        path
    };

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert!(swept.traces >= 1);
    assert!(!gone.exists(), "a readable trace with no hand-out goes");
    assert!(not_text.exists(), "a trace that is not UTF-8 is kept");
    assert!(large.exists(), "a trace past the largest read is kept");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Opening it as its owner is refused unless the test runs with the power to override
        // that, and then it is read like any other.
        let shut = std::fs::File::open(&unreadable).is_err();
        if shut {
            assert!(unreadable.exists(), "a trace that cannot be opened is kept");
            assert_eq!(swept.traces, 1);
        }
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

/// #1027: a file written to between the look that judged it stale and its removal is not the
/// file that was judged, and is kept.
#[test]
fn a_file_written_to_while_it_is_read_is_kept() {
    let (_d, root) = plane();
    let path = root.join(".charter/sessions/3.workspace");
    aged(&path, OLD);

    let swept = collect(
        &root,
        &[".charter", "sessions"],
        SystemTime::now(),
        |_| true,
        |_, written| {
            // What another process does while the sweep reads: an append to the same file.
            use std::io::Write;
            let mut file = std::fs::File::options().append(true).open(&path).unwrap();
            file.write_all(b"more\n").unwrap();
            Some(written)
        },
    );

    assert_eq!(swept, 0);
    assert_eq!(std::fs::read(&path).unwrap(), b"x\nmore\n");
}

/// #1027: another file renamed over the name while the sweep read the old one is not removed
/// in its place.
#[test]
fn a_file_renamed_over_the_name_while_it_is_read_is_kept() {
    let (_d, root) = plane();
    let path = root.join(".charter/sessions/3.workspace");
    aged(&path, OLD);
    let newer = root.join(".charter/sessions/elsewhere");
    aged(&newer, OLD);

    let swept = collect(
        &root,
        &[".charter", "sessions"],
        SystemTime::now(),
        |name| name == "3.workspace",
        |_, written| {
            // Same size and the same age: only which file it is tells them apart.
            std::fs::rename(&newer, &path).unwrap();
            Some(written)
        },
    );

    assert_eq!(swept, 0);
    assert!(path.exists(), "the file renamed over the name stands");
}

/// #1027: a link swapped in at the name while the sweep read the file is neither followed nor
/// removed: the look again opens the name without following a link, and finds no file.
#[cfg(unix)]
#[test]
fn a_link_swapped_in_while_the_file_is_read_is_kept_and_not_followed() {
    let (_d, root) = plane();
    let path = root.join(".charter/sessions/3.workspace");
    aged(&path, OLD);
    let elsewhere = root.join("kept-elsewhere");
    aged(&elsewhere, OLD);

    let swept = collect(
        &root,
        &[".charter", "sessions"],
        SystemTime::now(),
        |name| name == "3.workspace",
        |_, written| {
            std::fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
            Some(written)
        },
    );

    assert_eq!(swept, 0);
    assert!(path.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(elsewhere.is_file());
}

/// #1027 line 3: the trace scan reads in pieces, and a hand-out cut by any piece's edge is
/// still found. Every split of every event, at piece sizes down to one byte.
#[test]
fn a_hand_out_cut_by_the_edge_of_a_piece_is_still_found() {
    for event in crate::secrets::cmd::HANDED_OUT {
        let text =
            format!("{{\"event\":\"persona-use\"}}\n{{\"event\":\"{event}\",\"names\":[]}}\n");
        for chunk in 1..=text.len() {
            assert!(
                scan_for_hand_outs(text.as_bytes(), chunk),
                "{event} at pieces of {chunk}"
            );
        }
    }
    let plain = "{\"event\":\"persona-use\"}\n".repeat(50);
    for chunk in [1, 2, 7, 64, 4096] {
        assert!(!scan_for_hand_outs(plain.as_bytes(), chunk), "{chunk}");
    }
}

/// #1027 line 3: a character cut by a piece's edge is still text, and every fail-safe of the
/// whole read holds piece by piece: bytes that are not UTF-8 anywhere, a sequence left open at
/// the end, and a read that fails all keep the trace.
#[test]
fn a_streamed_scan_keeps_every_fail_safe_of_the_whole_read() {
    let text = "{\"note\":\"caf\u{e9} \u{1f512} \u{4e2d}\"}\n".repeat(20);
    for chunk in 1..=17 {
        assert!(!scan_for_hand_outs(text.as_bytes(), chunk), "{chunk}");
    }
    let mut bad = text.clone().into_bytes();
    bad.splice(40..40, [0xff_u8, 0xfe]);
    let mut open = text.into_bytes();
    open.extend_from_slice(&[0xf0, 0x9f]);
    for chunk in [1, 3, 64, 4096] {
        assert!(scan_for_hand_outs(bad.as_slice(), chunk), "{chunk}");
        assert!(scan_for_hand_outs(open.as_slice(), chunk), "{chunk}");
    }

    struct Fails;
    impl std::io::Read for Fails {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("refused"))
        }
    }
    assert!(scan_for_hand_outs(Fails, 64));
}

/// #1027 line 3: a trace that grows past the largest read while it is scanned is kept, as
/// one that was past it when it was opened is.
#[test]
fn a_trace_read_past_the_largest_read_is_kept() {
    use std::io::Read as _;
    let past = std::io::repeat(b'x').take(MOST_TRACE_BYTES + 1);
    assert!(scan_for_hand_outs(past, SCAN_CHUNK));
    let at = std::io::repeat(b'x').take(MOST_TRACE_BYTES);
    assert!(!scan_for_hand_outs(at, SCAN_CHUNK));
}

/// #1025: `workspace use` collects by the sweep's own rule. A chat the reopen record brings
/// back keeps its month-old pointer and lock, another chat's go, the tool gate's files are left
/// to the gate, and a per-terminal pointer goes by its age alone.
#[test]
fn a_selection_collects_by_the_rule_the_sweep_on_open_keeps() {
    let (_d, root) = plane();
    let record = crate::reopen::Record {
        chats: vec![crate::reopen::Chat {
            program: "claude".into(),
            number: Some(3),
            ..Default::default()
        }],
        dealt: 3,
        ..Default::default()
    };
    crate::reopen::write(&root, &record).unwrap();
    let sessions = root.join(".charter/sessions");
    let terminals = root.join(".charter/terminals");
    aged(&sessions.join("3.workspace"), OLD);
    aged(&sessions.join("3.lock"), OLD);
    aged(&sessions.join("2.workspace"), OLD);
    aged(&sessions.join("2.tools"), OLD);
    aged(&sessions.join("4.workspace"), YOUNG);
    aged(&terminals.join("pane-old.workspace"), OLD);
    aged(&terminals.join("pane-new.workspace"), YOUNG);

    assert_eq!(on_select(&root, SystemTime::now()), 2);

    assert!(
        sessions.join("3.workspace").exists(),
        "a returning chat's pointer"
    );
    assert!(sessions.join("3.lock").exists(), "and its lock");
    assert!(!sessions.join("2.workspace").exists());
    assert!(
        sessions.join("2.tools").exists(),
        "the gate's, collected in its order"
    );
    assert!(sessions.join("4.workspace").exists());
    assert!(!terminals.join("pane-old.workspace").exists());
    assert!(terminals.join("pane-new.workspace").exists());
}

/// #1025: where the reopen record cannot say which chats come back, a selection keeps every
/// session marker, as the sweep on open does. A per-terminal pointer is no chat's, and goes
/// by its age.
#[test]
fn a_selection_keeps_every_session_marker_when_the_record_cannot_say_who_comes_back() {
    let (_d, root) = plane();
    a_record_saying(&root, "{not json");
    aged(&root.join(".charter/terminals/pane.workspace"), OLD);

    assert_eq!(on_select(&root, SystemTime::now()), 1);
    assert!(root.join(".charter/sessions/3.workspace").exists());
    assert!(!root.join(".charter/terminals/pane.workspace").exists());
}

/// #1025: the per-terminal store is opened as every other store is, so one reached through a
/// link is not swept.
#[cfg(unix)]
#[test]
fn a_terminal_store_reached_through_a_link_is_not_swept() {
    let (_d, root) = plane();
    let (_o, elsewhere) = plane();
    aged(&elsewhere.join("pane.workspace"), OLD);
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join(".charter/terminals")).unwrap();

    assert_eq!(on_select(&root, SystemTime::now()), 0);
    assert!(elsewhere.join("pane.workspace").exists());
}

#[test]
fn a_commit_gate_cooldown_untouched_for_thirty_days_is_collected_unless_its_session_is_live() {
    let (_d, root) = plane();
    let gate = root.join(".charter/commit-gate");
    aged(&gate.join("ended-uuid"), OLD);
    aged(&gate.join("9f1c-uuid"), OLD);
    aged(&gate.join("recent-uuid"), YOUNG);
    // Not a name the gate makes.
    aged(&gate.join(".hidden"), OLD);
    std::fs::create_dir_all(gate.join("dir-uuid")).unwrap();

    let swept = sweep(&root, SystemTime::now(), &["9f1c-uuid".into()]);

    assert_eq!(swept.gates, 1);
    assert!(!gate.join("ended-uuid").exists());
    for kept in ["9f1c-uuid", "recent-uuid", ".hidden", "dir-uuid"] {
        assert!(gate.join(kept).exists(), "{kept} was removed");
    }
}

#[test]
fn a_nudge_marker_untouched_for_thirty_days_is_collected_unless_its_session_is_live() {
    let (_d, root) = plane();
    let nudges = root.join(".charter/ws-edit-nudge");
    aged(&nudges.join("ended-uuid-alpha"), OLD);
    aged(&nudges.join("9f1c-uuid-alpha"), OLD);
    aged(&nudges.join("9f1c-uuid-beta"), OLD);
    aged(&nudges.join("recent-uuid-alpha"), YOUNG);
    // Session 9f1c is not session 9f1cd.
    aged(&nudges.join("9f1cd-alpha"), OLD);

    let swept = sweep(&root, SystemTime::now(), &["9f1c-uuid".into()]);

    assert_eq!(swept.nudges, 2);
    assert!(!nudges.join("ended-uuid-alpha").exists());
    assert!(!nudges.join("9f1cd-alpha").exists());
    for kept in ["9f1c-uuid-alpha", "9f1c-uuid-beta", "recent-uuid-alpha"] {
        assert!(nudges.join(kept).exists(), "{kept} was removed");
    }
}

#[test]
fn a_workspace_s_saved_line_untouched_for_thirty_days_is_collected_unless_its_chat_is_live() {
    let (_d, root) = plane();
    let alpha = root.join("workspaces/alpha/.charter/sessions");
    let beta = root.join("workspaces/beta/.charter/sessions");
    aged(&alpha.join("2.saved"), OLD);
    aged(&alpha.join("3.saved"), OLD);
    aged(&beta.join("4.saved"), YOUNG);
    aged(&beta.join("5.saved"), OLD);
    // Only the saved lines: the rest of a workspace's sessions folder is not this sweep's.
    aged(&alpha.join("2.workspace"), OLD);
    aged(&alpha.join("x.saved"), OLD);

    let swept = sweep(&root, SystemTime::now(), &["3".into()]);

    assert_eq!(swept.saved, 2);
    assert!(!alpha.join("2.saved").exists());
    assert!(!beta.join("5.saved").exists());
    assert!(alpha.join("3.saved").exists());
    assert!(beta.join("4.saved").exists());
    assert!(alpha.join("2.workspace").exists());
    assert!(alpha.join("x.saved").exists());
}

#[cfg(unix)]
#[test]
fn a_workspace_reached_through_a_link_has_nothing_swept() {
    let (_d, root) = plane();
    let elsewhere = tempfile::tempdir().unwrap();
    let theirs = elsewhere.path().join(".charter/sessions/2.saved");
    aged(&theirs, OLD);
    std::fs::create_dir_all(root.join("workspaces")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.join("workspaces/alpha")).unwrap();

    assert_eq!(sweep(&root, SystemTime::now(), &[]).saved, 0);

    assert!(theirs.exists());
}

/// A file of `session`'s ephemeral memory for `persona`, last written `age` ago, with its
/// folders as old.
fn ephemeral(root: &Path, session: &str, persona: &str, file: &str, age: Duration) -> PathBuf {
    let path = crate::recall::ephemeral_dir(root, session, persona).join(file);
    aged(&path, age);
    let store = root.join(".charter/persona-state/ephemeral");
    for dir in path.ancestors().skip(1).take_while(|dir| *dir != store) {
        std::fs::File::open(dir)
            .unwrap()
            .set_modified(SystemTime::now() - age)
            .unwrap();
    }
    path
}

#[test]
fn a_session_s_ephemeral_memory_untouched_for_thirty_days_is_collected_whole() {
    let (_d, root) = plane();
    let store = root.join(".charter/persona-state/ephemeral");
    ephemeral(&root, "ended", "devops", "scratch.md", OLD);
    ephemeral(&root, "ended", "devops", "MEMORY.md", OLD);
    ephemeral(&root, "ended", "_shared", "note.md", OLD);
    ephemeral(&root, "recent", "devops", "scratch.md", YOUNG);

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.ephemeral, 1);
    assert!(!store.join("ended").exists());
    assert!(store.join("recent/devops/scratch.md").exists());
}

#[test]
fn a_live_session_keeps_its_ephemeral_memory_however_old() {
    let (_d, root) = plane();
    let kept = ephemeral(&root, "3", "devops", "scratch.md", OLD);

    let swept = sweep(&root, SystemTime::now(), &["3".into()]);

    assert_eq!(swept.ephemeral, 0);
    assert!(kept.exists());
}

#[test]
fn one_fresh_file_keeps_a_session_s_whole_ephemeral_memory() {
    let (_d, root) = plane();
    let old = ephemeral(&root, "ended", "devops", "old.md", OLD);
    ephemeral(&root, "ended", "ops", "old.md", OLD);
    // Written yesterday, in a folder whose own time says a month: the file's time decides.
    let fresh = old.with_file_name("fresh.md");
    aged(&fresh, Duration::from_secs(24 * 60 * 60));
    std::fs::File::open(fresh.parent().unwrap())
        .unwrap()
        .set_modified(SystemTime::now() - OLD)
        .unwrap();

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.ephemeral, 0);
    assert!(old.exists());
    assert!(fresh.exists());
}

#[test]
fn an_ephemeral_session_holding_what_charter_never_writes_there_is_kept_whole() {
    let (_d, root) = plane();
    // A folder below a persona's: not the shape `ephemeral_dir` makes.
    let deep = ephemeral(&root, "ended", "devops", "nested/deep.md", OLD);
    // A file directly in a session's folder.
    let stray = root.join(".charter/persona-state/ephemeral/stray/loose.md");
    aged(&stray, OLD);
    std::fs::File::open(stray.parent().unwrap())
        .unwrap()
        .set_modified(SystemTime::now() - OLD)
        .unwrap();

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.ephemeral, 0);
    assert!(deep.exists());
    assert!(stray.exists());
}

/// A name that is not UTF-8 is not one charter writes, so it keeps its session whole, the
/// files beside it too. Linux only: APFS refuses such a name.
#[cfg(target_os = "linux")]
#[test]
fn an_ephemeral_session_holding_a_name_that_is_not_utf8_is_kept_whole() {
    use std::os::unix::ffi::OsStrExt as _;
    let (_d, root) = plane();
    let ours = ephemeral(&root, "ended", "devops", "scratch.md", OLD);
    let odd = ours
        .parent()
        .unwrap()
        .join(std::ffi::OsStr::from_bytes(b"odd-\xff.md"));
    aged(&odd, OLD);
    std::fs::File::open(ours.parent().unwrap())
        .unwrap()
        .set_modified(SystemTime::now() - OLD)
        .unwrap();

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.ephemeral, 0);
    assert!(ours.exists());
    assert!(odd.exists());
}

#[cfg(unix)]
#[test]
fn an_ephemeral_session_holding_a_link_is_kept_and_the_link_not_followed() {
    let (_d, root) = plane();
    let elsewhere = tempfile::tempdir().unwrap();
    let theirs = elsewhere.path().join("theirs.md");
    aged(&theirs, OLD);
    let ours = ephemeral(&root, "ended", "devops", "scratch.md", OLD);
    std::os::unix::fs::symlink(
        elsewhere.path(),
        ours.parent().unwrap().parent().unwrap().join("ops"),
    )
    .unwrap();

    let swept = sweep(&root, SystemTime::now(), &[]);

    assert_eq!(swept.ephemeral, 0);
    assert!(ours.exists());
    assert!(theirs.exists());
}

#[test]
fn opening_a_project_keeps_the_per_session_stores_of_every_chat_it_brings_back() {
    let (_d, root) = plane();
    let record = crate::reopen::Record {
        chats: vec![crate::reopen::Chat {
            program: "claude".into(),
            number: Some(3),
            resume: Some(crate::harness::SessionId::new("9f1c-uuid").unwrap()),
            ..Default::default()
        }],
        dealt: 3,
        ..Default::default()
    };
    crate::reopen::write(&root, &record).unwrap();
    let state = root.join(".charter");
    let kept = [
        state.join("commit-gate/9f1c-uuid"),
        state.join("ws-edit-nudge/9f1c-uuid-alpha"),
        root.join("workspaces/alpha/.charter/sessions/3.saved"),
    ];
    for file in &kept {
        aged(file, OLD);
    }
    let scratch = ephemeral(&root, "3", "devops", "scratch.md", OLD);
    let gone = [
        state.join("commit-gate/ended-uuid"),
        state.join("ws-edit-nudge/ended-uuid-alpha"),
        root.join("workspaces/alpha/.charter/sessions/2.saved"),
    ];
    for file in &gone {
        aged(file, OLD);
    }
    ephemeral(&root, "2", "devops", "scratch.md", OLD);

    let swept = on_open(&root, SystemTime::now());

    assert_eq!(
        (swept.gates, swept.nudges, swept.saved, swept.ephemeral),
        (1, 1, 1, 1)
    );
    for file in kept.iter().chain([&scratch]) {
        assert!(file.exists(), "{} was removed", file.display());
    }
    for file in &gone {
        assert!(!file.exists(), "{} was kept", file.display());
    }
}

#[test]
fn a_reopen_record_that_cannot_be_read_leaves_every_per_session_store_alone() {
    let (_d, root) = plane();
    a_record_saying(&root, "{not json");
    let state = root.join(".charter");
    let files = [
        state.join("commit-gate/3"),
        state.join("ws-edit-nudge/3-alpha"),
        root.join("workspaces/alpha/.charter/sessions/3.saved"),
        ephemeral(&root, "3", "devops", "scratch.md", OLD),
    ];
    for file in &files[..3] {
        aged(file, OLD);
    }

    let swept = on_open(&root, SystemTime::now());

    assert_eq!(
        (swept.gates, swept.nudges, swept.saved, swept.ephemeral),
        (0, 0, 0, 0)
    );
    for file in &files {
        assert!(file.exists(), "{} was removed", file.display());
    }
}
