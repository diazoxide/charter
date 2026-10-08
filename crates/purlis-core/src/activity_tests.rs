use std::path::{Path, PathBuf};

use super::*;
use crate::dispatchrecord::{
    self, Asker, Changed, ChatRef, Ending, Mode, Opening, Outcome, Place, Report, Taken, Worker,
};
use crate::dispatchtalk::Kind as Sent;

fn project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

fn at(time: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(time)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

fn chat(number: u32, name: &str) -> ChatRef {
    ChatRef {
        chat: number,
        id: Some(format!("01K6CHAT{number:018}")),
        name: name.to_owned(),
        persona: None,
    }
}

fn steward() -> ChatRef {
    chat(3, "steward 3")
}

/// `asker` dispatches `worker` as the task `task`, with a brief.
fn a_task(asker: &ChatRef, worker: &ChatRef, task: &str, brief: &str) -> Opening {
    Opening {
        mode: Mode::Task,
        asker: Asker {
            chat: asker.clone(),
            ..Asker::default()
        },
        persona: None,
        worker: Worker {
            chat: worker.clone(),
            ..Worker::default()
        },
        task: Some(task.to_owned()),
        place: Place::default(),
        brief: brief.to_owned(),
        report_owed: true,
    }
}

fn started(root: &Path, asker: &ChatRef, worker: &ChatRef, task: &str, when: &str) -> String {
    dispatchrecord::open(root, a_task(asker, worker, task, "the brief"), at(when))
        .unwrap()
        .id
}

fn says(root: &Path, id: &str, kind: Sent, text: &str, when: &str) {
    let taken = dispatchrecord::said(root, id, kind, text, at(when)).unwrap();
    assert!(matches!(taken, Taken::Kept(_)), "kept: {taken:?}");
}

/// The timeline as read the afternoon of the day these tests' tasks run.
fn timeline(root: &Path, session: &ChatRef) -> Timeline {
    super::timeline(root, session, at("2026-10-08T12:00:00Z"))
}

/// The one line that says what a task's record did not keep: how many, and why.
fn unkept(lines: &[Line]) -> Vec<(u32, Why)> {
    lines.iter().filter_map(|line| line.unkept).collect()
}

fn reports(root: &Path, id: &str, outcome: Outcome, text: &str, said: Option<&str>, when: &str) {
    let ending = Ending {
        report: Some(Report {
            outcome,
            text: text.to_owned(),
            changed: Changed {
                said: said.map(str::to_owned),
                ..Changed::default()
            },
        }),
        usage: None,
    };
    assert!(dispatchrecord::close(root, id, ending, at(when)).unwrap());
}

/// Each line as `time kind from>to: text`, the time by its clock alone.
fn read(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            format!(
                "{} {} {}>{}: {}",
                &line.at[11..19],
                line.kind.word(),
                line.from.name,
                line.to.name,
                line.text
            )
        })
        .collect()
}

// ----- recording ----------------------------------------------------------------------------

#[test]
fn a_message_is_kept_on_its_task_s_record_with_its_time_and_kind() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );

    let kept = dispatchrecord::said(
        &root,
        &id,
        Sent::Question,
        "Which host?",
        at("2026-10-08T09:01:00Z"),
    )
    .unwrap();

    let record = dispatchrecord::read(&root, &id).expect("it reads");
    assert_eq!(kept, Taken::Kept(record.clone()));
    assert_eq!(record.messages, 1);
    assert_eq!(
        record.talk,
        vec![dispatchrecord::Said {
            at: "2026-10-08T09:01:00+00:00".to_owned(),
            kind: Sent::Question,
            text: "Which host?".to_owned(),
            by: None,
            unread: false,
        }]
    );
    assert!(dispatchrecord::sound(&record));
}

#[test]
fn nothing_is_kept_on_a_dispatch_that_has_ended() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    reports(
        &root,
        &id,
        Outcome::Done,
        "Done.",
        None,
        "2026-10-08T09:05:00Z",
    );

    let kept =
        dispatchrecord::said(&root, &id, Sent::Note, "late", at("2026-10-08T09:06:00Z")).unwrap();

    assert_eq!(kept, Taken::Nothing);
    let record = dispatchrecord::read(&root, &id).unwrap();
    assert_eq!((record.messages, record.talk.len()), (0, 0));
}

#[test]
fn a_message_past_what_a_record_keeps_is_counted_and_its_text_is_not_kept() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    // Full-size messages: the bytes run out long before the count does.
    let long = "x".repeat(dispatchrecord::MOST_MESSAGE_BYTES);
    let fit = dispatchrecord::MOST_SAID_BYTES / dispatchrecord::MOST_MESSAGE_BYTES;
    for _ in 0..fit {
        says(&root, &id, Sent::Note, &long, "2026-10-08T09:01:00Z");
    }

    let over =
        dispatchrecord::said(&root, &id, Sent::Note, &long, at("2026-10-08T09:02:00Z")).unwrap();

    let record = dispatchrecord::read(&root, &id).expect("the record still reads back");
    assert_eq!(over, Taken::Counted(record.clone()), "its text is not kept");
    assert_eq!(record.talk.len(), fit);
    assert_eq!(record.messages as usize, fit + 1, "and it is still counted");
    assert!(dispatchrecord::sound(&record));
    // The timeline says how many it does not have, and that it was the size that ran out.
    assert_eq!(unkept(&timeline(&root, &steward()).lines), [(1, Why::Size)]);
}

#[test]
fn no_more_messages_are_kept_than_a_record_holds() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    for _ in 0..dispatchrecord::MOST_SAID {
        says(&root, &id, Sent::Note, "on it", "2026-10-08T09:01:00Z");
    }

    let over =
        dispatchrecord::said(&root, &id, Sent::Note, "on it", at("2026-10-08T09:02:00Z")).unwrap();

    let record = dispatchrecord::read(&root, &id).unwrap();
    assert_eq!(over, Taken::Counted(record.clone()));
    assert_eq!(record.talk.len(), dispatchrecord::MOST_SAID);
    assert_eq!(record.messages as usize, dispatchrecord::MOST_SAID + 1);
    assert_eq!(
        unkept(&timeline(&root, &steward()).lines),
        [(1, Why::Count)]
    );
}

// ----- the timeline -------------------------------------------------------------------------

#[test]
fn every_message_between_a_session_and_its_task_appears_once_in_order() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    says(
        &root,
        &id,
        Sent::Note,
        "Reading the config.",
        "2026-10-08T09:01:00Z",
    );
    says(
        &root,
        &id,
        Sent::Question,
        "Which host?",
        "2026-10-08T09:02:00Z",
    );
    says(&root, &id, Sent::Answer, "prod-2.", "2026-10-08T09:03:00Z");
    says(
        &root,
        &id,
        Sent::FollowUp,
        "Check the cache too.",
        "2026-10-08T09:04:00Z",
    );
    reports(
        &root,
        &id,
        Outcome::Done,
        "Both are healthy.",
        None,
        "2026-10-08T09:05:00Z",
    );

    let found = timeline(&root, &steward());

    assert_eq!(
        read(&found.lines),
        [
            "09:00:00 dispatched steward 3>talk: the brief",
            "09:01:00 note talk>steward 3: Reading the config.",
            "09:02:00 question talk>steward 3: Which host?",
            "09:03:00 answer steward 3>talk: prod-2.",
            "09:04:00 follow-up steward 3>talk: Check the cache too.",
            "09:05:00 report talk>steward 3: Both are healthy.",
        ]
    );
    assert_eq!((unkept(&found.lines), found.refused), (vec![], 0));
    // A line is named by its dispatch and its place in it, so it is drawn once.
    let names: Vec<(&str, u32)> = found
        .lines
        .iter()
        .map(|line| (line.dispatch.as_str(), line.n))
        .collect();
    assert_eq!(names, (0..6).map(|n| (id.as_str(), n)).collect::<Vec<_>>());
    assert_eq!(found.lines[5].outcome, Some(Outcome::Done));
    assert!(found.lines.iter().all(|line| line.task == "talk"));
}

#[test]
fn two_tasks_of_one_session_are_one_timeline_in_time_order() {
    let (_d, root) = project();
    let talk = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    let lint = started(
        &root,
        &steward(),
        &chat(8, "lint"),
        "lint",
        "2026-10-08T09:00:30Z",
    );
    says(
        &root,
        &lint,
        Sent::Note,
        "40 files.",
        "2026-10-08T09:01:00Z",
    );
    says(
        &root,
        &talk,
        Sent::Note,
        "Half way.",
        "2026-10-08T09:02:00Z",
    );
    reports(
        &root,
        &lint,
        Outcome::Failed,
        "3 errors.",
        None,
        "2026-10-08T09:03:00Z",
    );

    assert_eq!(
        read(&timeline(&root, &steward()).lines),
        [
            "09:00:00 dispatched steward 3>talk: the brief",
            "09:00:30 dispatched steward 3>lint: the brief",
            "09:01:00 note lint>steward 3: 40 files.",
            "09:02:00 note talk>steward 3: Half way.",
            "09:03:00 report lint>steward 3: 3 errors.",
        ]
    );
}

#[test]
fn a_session_s_timeline_holds_the_tasks_of_its_tasks_and_no_other_session_s() {
    let (_d, root) = project();
    let (talk, deep) = (chat(7, "talk"), chat(9, "dig"));
    started(&root, &steward(), &talk, "talk", "2026-10-08T09:00:00Z");
    let below = started(&root, &talk, &deep, "dig", "2026-10-08T09:01:00Z");
    says(
        &root,
        &below,
        Sent::Note,
        "Found it.",
        "2026-10-08T09:02:00Z",
    );
    // Another session's task, and what it said.
    let other = started(
        &root,
        &chat(4, "planner 4"),
        &chat(11, "plan"),
        "plan",
        "2026-10-08T09:01:30Z",
    );
    says(
        &root,
        &other,
        Sent::Note,
        "Not yours.",
        "2026-10-08T09:02:30Z",
    );

    let found = timeline(&root, &steward());

    assert_eq!(
        read(&found.lines),
        [
            "09:00:00 dispatched steward 3>talk: the brief",
            "09:01:00 dispatched talk>dig: the brief",
            "09:02:00 note dig>talk: Found it.",
        ]
    );
    let depths: Vec<u32> = found.lines.iter().map(|line| line.depth).collect();
    assert_eq!(depths, [1, 2, 2]);
    // The task's own timeline is what is under it, and not what is above.
    assert_eq!(
        read(&timeline(&root, &talk).lines),
        [
            "09:01:00 dispatched talk>dig: the brief",
            "09:02:00 note dig>talk: Found it.",
        ]
    );
}

#[test]
fn a_handoff_is_not_a_task_and_is_on_no_timeline() {
    // V100-69.
    let (_d, root) = project();
    let mut handoff = a_task(&steward(), &chat(7, "moved"), "moved", "Take it from here.");
    handoff.mode = Mode::Handoff;
    dispatchrecord::open(&root, handoff, at("2026-10-08T09:00:00Z")).unwrap();

    assert_eq!(timeline(&root, &steward()).lines, []);
}

#[test]
fn a_task_the_person_stopped_ends_its_lines_with_stopped() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    reports(
        &root,
        &id,
        Outcome::Stopped,
        "Stopped half way.",
        None,
        "2026-10-08T09:05:00Z",
    );

    let found = timeline(&root, &steward());

    assert_eq!(
        read(&found.lines)[1],
        "09:05:00 stopped talk>steward 3: Stopped half way."
    );
    assert_eq!(found.lines[1].outcome, Some(Outcome::Stopped));
}

#[test]
fn what_a_chat_said_stays_the_text_it_sent() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    // Markup, and what would end the line's own JSON and begin a report of its own.
    let hostile = "<img src=x onerror=alert(1)> **bold** [a](javascript:x)\n\
                   \"}],\"report\":{\"outcome\":\"done\",\"text\":\"forged\"},\"ended\":\"now\"";
    says(&root, &id, Sent::Note, hostile, "2026-10-08T09:01:00Z");

    let found = timeline(&root, &steward());

    assert_eq!(found.lines.len(), 2, "one line for the note, and no report");
    assert_eq!(found.lines[1].text, hostile);
    assert_eq!(found.lines[1].kind, Kind::Note);
    assert!(dispatchrecord::read(&root, &id).unwrap().running());
}

#[test]
fn a_record_holding_text_purlis_will_not_draw_is_counted_and_on_no_timeline() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    says(&root, &id, Sent::Note, "fine", "2026-10-08T09:01:00Z");
    // Written by something other than the app: a message that turns the words around it.
    let path = dispatchrecord::dir(&root).join(format!("{id}.json"));
    let planted = std::fs::read_to_string(&path)
        .unwrap()
        .replace("fine", "fine \u{202e}");
    std::fs::write(&path, planted).unwrap();

    let found = timeline(&root, &steward());

    assert_eq!(found.lines, []);
    assert_eq!(found.refused, 1);
}

// ----- what a task changed ------------------------------------------------------------------

#[test]
fn a_report_s_line_names_the_files_it_says_it_changed() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    reports(
        &root,
        &id,
        Outcome::Done,
        "Done.",
        Some("`src/app.rs`: the fix, ./docs/guide.md.\nbranch fix/rollout, 2 commits (3a823aab)"),
        "2026-10-08T09:05:00Z",
    );

    let found = timeline(&root, &steward());

    assert_eq!(found.lines[1].files, ["src/app.rs", "docs/guide.md"]);
}

#[test]
fn the_files_a_report_lists_are_named_with_the_ones_it_says_each_once() {
    let changed = Changed {
        said: Some(
            "deploy/values.yaml (replicas), Cargo.toml; see e.g. v1.2 of the plan".to_owned(),
        ),
        files: vec!["deploy/values.yaml".to_owned(), "README.md".to_owned()],
        commits: vec!["3a823aab".to_owned()],
        branch: Some("fix/rollout".to_owned()),
    };

    assert_eq!(
        paths_named(&changed),
        ["deploy/values.yaml", "README.md", "Cargo.toml"]
    );
    assert_eq!(paths_named(&Changed::default()), Vec::<String>::new());
}

// ----- fix round 1: who said it, what is not kept, where a task worked ------------------------

/// Rewrites the stored record `id` through its JSON, as something other than the app would.
fn planted(root: &Path, id: &str, change: impl FnOnce(&mut serde_json::Value)) {
    let path = dispatchrecord::dir(root).join(format!("{id}.json"));
    let mut record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut record);
    std::fs::write(&path, serde_json::to_string_pretty(&record).unwrap()).unwrap();
}

#[test]
fn a_task_the_person_dispatched_says_so_on_its_first_line_and_on_no_other() {
    // V100-70: "Ask a persona" from a chat's tab. The brief is the person's, not that chat's.
    let (_d, root) = project();
    let mut asked = a_task(&steward(), &chat(7, "talk"), "talk", "Is prod healthy?");
    asked.asker.by_person = true;
    let id = dispatchrecord::open(&root, asked, at("2026-10-08T09:00:00Z"))
        .unwrap()
        .id;
    says(&root, &id, Sent::Note, "Looking.", "2026-10-08T09:01:00Z");
    // And a task the chat dispatched itself.
    started(
        &root,
        &steward(),
        &chat(8, "lint"),
        "lint",
        "2026-10-08T09:02:00Z",
    );

    let found = timeline(&root, &steward());

    let by_person: Vec<bool> = found.lines.iter().map(|line| line.by_person).collect();
    assert_eq!(by_person, [true, false, false]);
    // The asking chat is still the chat the line belongs to.
    assert_eq!(found.lines[0].from.name, "steward 3");
}

#[test]
fn once_a_message_is_not_kept_no_later_one_is_so_the_kept_ones_are_the_first() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    let long = "x".repeat(dispatchrecord::MOST_MESSAGE_BYTES);
    let fit = dispatchrecord::MOST_SAID_BYTES / dispatchrecord::MOST_MESSAGE_BYTES;
    // One byte short of full, so a small message would still fit by size.
    for _ in 0..fit - 1 {
        says(&root, &id, Sent::Note, &long, "2026-10-08T09:01:00Z");
    }
    says(&root, &id, Sent::Note, &long[1..], "2026-10-08T09:01:00Z");
    // A question too long to fit is left out.
    let question = dispatchrecord::said(
        &root,
        &id,
        Sent::Question,
        &long,
        at("2026-10-08T09:02:00Z"),
    )
    .unwrap();
    assert!(matches!(question, Taken::Counted(_)), "{question:?}");

    // Its answer is one byte and would fit: it is not kept either.
    let answer =
        dispatchrecord::said(&root, &id, Sent::Answer, "y", at("2026-10-08T09:03:00Z")).unwrap();

    assert!(matches!(answer, Taken::Counted(_)), "{answer:?}");
    let record = dispatchrecord::read(&root, &id).unwrap();
    assert_eq!(record.talk.len(), fit);
    assert!(record.talk.iter().all(|said| said.kind == Sent::Note));
    // One line for the two, after the last kept and before anything later.
    let found = timeline(&root, &steward());
    assert_eq!(unkept(&found.lines), [(2, Why::Size)]);
    let last = found.lines.last().expect("the line");
    assert_eq!((last.kind, last.by_purlis), (Kind::Unkept, true));
    assert_eq!(last.n as usize, fit + 1);
}

#[test]
fn a_record_from_before_messages_were_kept_says_so_and_never_says_a_cap() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    // As a build before this one left it: four messages counted, and no text.
    planted(&root, &id, |record| record["messages"] = 4.into());

    assert_eq!(
        unkept(&timeline(&root, &steward()).lines),
        [(4, Why::Before)]
    );

    // Still running under this build: what it says next is counted, and not kept, so the
    // record never holds the end of a conversation without its start.
    let next =
        dispatchrecord::said(&root, &id, Sent::Note, "more", at("2026-10-08T09:01:00Z")).unwrap();
    assert!(matches!(next, Taken::Counted(_)), "{next:?}");
    reports(
        &root,
        &id,
        Outcome::Done,
        "Done.",
        None,
        "2026-10-08T09:05:00Z",
    );
    let found = timeline(&root, &steward());
    assert_eq!(unkept(&found.lines), [(5, Why::Before)]);
    // In the task's place: after its dispatch, before its report.
    let kinds: Vec<Kind> = found.lines.iter().map(|line| line.kind).collect();
    assert_eq!(kinds, [Kind::Dispatched, Kind::Unkept, Kind::Report]);
}

#[test]
fn what_was_said_is_taken_out_thirty_days_after_the_task_ended_and_the_record_stays() {
    // D-1495-12.
    let (_d, root) = project();
    let old = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-09-01T09:00:00Z",
    );
    says(
        &root,
        &old,
        Sent::Question,
        "Which host?",
        "2026-09-01T09:01:00Z",
    );
    says(&root, &old, Sent::Answer, "prod-2.", "2026-09-01T09:02:00Z");
    reports(
        &root,
        &old,
        Outcome::Done,
        "Healthy.",
        None,
        "2026-09-01T09:05:00Z",
    );
    // One that started as long ago and has not ended keeps its words.
    let running = started(
        &root,
        &steward(),
        &chat(8, "watch"),
        "watch",
        "2026-09-01T09:00:30Z",
    );
    says(
        &root,
        &running,
        Sent::Note,
        "Still up.",
        "2026-09-01T09:03:00Z",
    );

    // A day short of thirty: nothing goes.
    assert_eq!(
        dispatchrecord::expire_talk(&root, at("2026-09-30T09:05:00Z")),
        0
    );
    assert_eq!(
        dispatchrecord::read(&root, &old).unwrap().talk[0].text,
        "Which host?"
    );

    // Thirty days on, read as a timeline: the words are gone from the file itself.
    let found = super::timeline(&root, &steward(), at("2026-10-01T09:05:00Z"));

    let record = dispatchrecord::read(&root, &old).expect("the record stays");
    assert_eq!(record.messages, 2);
    let kept: Vec<(&str, Sent, &str)> = record
        .talk
        .iter()
        .map(|said| (said.at.as_str(), said.kind, said.text.as_str()))
        .collect();
    assert_eq!(
        kept,
        [
            ("2026-09-01T09:01:00+00:00", Sent::Question, ""),
            ("2026-09-01T09:02:00+00:00", Sent::Answer, ""),
        ]
    );
    assert!(dispatchrecord::sound(&record));
    // The brief and the report are the record's, and stay.
    assert_eq!(
        record.report.as_ref().map(|report| report.text.as_str()),
        Some("Healthy.")
    );
    let stored =
        std::fs::read_to_string(dispatchrecord::dir(&root).join(format!("{old}.json"))).unwrap();
    assert!(!stored.contains("prod-2"), "{stored}");
    // Each message keeps its line, and says its words are no longer kept.
    let lines: Vec<(Kind, &str, bool)> = found
        .lines
        .iter()
        .map(|line| (line.kind, line.text.as_str(), line.expired))
        .collect();
    assert_eq!(
        lines,
        [
            (Kind::Dispatched, "the brief", false),
            (Kind::Dispatched, "the brief", false),
            (Kind::Question, "", true),
            (Kind::Answer, "", true),
            (Kind::Note, "Still up.", false),
            (Kind::Report, "Healthy.", false),
        ]
    );
    assert!(
        unkept(&found.lines).is_empty(),
        "every message has its line"
    );
    // Done once: a second look changes nothing.
    assert_eq!(
        dispatchrecord::expire_talk(&root, at("2026-10-02T09:05:00Z")),
        0
    );
}

#[test]
fn an_ending_purlis_wrote_is_purlis_s_line_and_a_task_s_own_report_is_the_task_s() {
    let (_d, root) = project();
    let own = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    reports(
        &root,
        &own,
        Outcome::Failed,
        "The probe timed out.",
        None,
        "2026-10-08T09:05:00Z",
    );
    let stopped = started(
        &root,
        &steward(),
        &chat(8, "lint"),
        "lint",
        "2026-10-08T09:00:10Z",
    );
    reports(
        &root,
        &stopped,
        Outcome::Stopped,
        crate::handback::STOPPED,
        None,
        "2026-10-08T09:06:00Z",
    );
    started(
        &root,
        &steward(),
        &chat(9, "dig"),
        "dig",
        "2026-10-08T09:00:20Z",
    );
    // Its chat went without reporting, and the app settled it.
    dispatchrecord::settle(&root, |worker| worker.chat != 9, at("2026-10-08T09:07:00Z"));

    let found = timeline(&root, &steward());

    let endings: Vec<(&str, bool)> = found
        .lines
        .iter()
        .filter(|line| line.outcome.is_some())
        .map(|line| (line.text.as_str(), line.by_purlis))
        .collect();
    assert_eq!(
        endings,
        [
            ("The probe timed out.", false),
            ("stopped by the operator", true),
            ("ended without a report", true),
        ]
    );
}

#[test]
fn a_clock_that_steps_back_never_puts_an_answer_before_its_question() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    says(
        &root,
        &id,
        Sent::Question,
        "Which host?",
        "2026-10-08T09:10:00Z",
    );
    // The clock was set back five minutes before the answer.
    says(&root, &id, Sent::Answer, "prod-2.", "2026-10-08T09:05:00Z");
    // Another task's line, between the two by the clock.
    let lint = started(
        &root,
        &steward(),
        &chat(8, "lint"),
        "lint",
        "2026-10-08T09:00:30Z",
    );
    says(
        &root,
        &lint,
        Sent::Note,
        "40 files.",
        "2026-10-08T09:07:00Z",
    );

    assert_eq!(
        read(&timeline(&root, &steward()).lines),
        [
            "09:00:00 dispatched steward 3>talk: the brief",
            "09:00:30 dispatched steward 3>lint: the brief",
            "09:07:00 note lint>steward 3: 40 files.",
            "09:10:00 question talk>steward 3: Which host?",
            // Its own time is still said; its place is after its question.
            "09:05:00 answer steward 3>talk: prod-2.",
        ]
    );
}

#[test]
fn only_this_session_s_tasks_are_counted_as_not_listed() {
    let (_d, root) = project();
    let talk = chat(7, "talk");
    started(&root, &steward(), &talk, "talk", "2026-10-08T09:00:00Z");
    let mine = started(
        &root,
        &steward(),
        &chat(8, "lint"),
        "lint",
        "2026-10-08T09:01:00Z",
    );
    let below = started(&root, &talk, &chat(9, "dig"), "dig", "2026-10-08T09:02:00Z");
    let theirs = started(
        &root,
        &chat(4, "planner 4"),
        &chat(11, "plan"),
        "plan",
        "2026-10-08T09:03:00Z",
    );
    // Three records purlis will not draw: the session's own, one under its task, another's.
    for id in [&mine, &below, &theirs] {
        planted(&root, id, |record| {
            record["brief"] = "turned \u{202e}".into()
        });
    }

    assert_eq!(timeline(&root, &steward()).refused, 2);
    assert_eq!(timeline(&root, &chat(4, "planner 4")).refused, 1);
    assert_eq!(timeline(&root, &chat(12, "idle 12")).refused, 0);
}

#[test]
fn two_tasks_sent_to_one_folder_share_a_place_and_a_task_on_its_own_branch_shares_none() {
    let (_d, root) = project();
    let sent = |worker: u32, task: &str, folder: &str, when: &str| {
        let mut opening = a_task(&steward(), &chat(worker, task), task, "the brief");
        opening.place = Place {
            workspace: Some("alpha".to_owned()),
            folder: Some(folder.to_owned()),
            worktree: None,
        };
        dispatchrecord::open(&root, opening, at(when)).unwrap();
    };
    sent(7, "talk", "workspaces/alpha/svc", "2026-10-08T09:00:00Z");
    sent(8, "talk", "workspaces/alpha/svc", "2026-10-08T09:00:10Z");
    sent(
        9,
        "lint",
        "workspaces/alpha/.worktrees/svc/lint-0def",
        "2026-10-08T09:00:20Z",
    );

    let found = timeline(&root, &steward());

    let places: Vec<&str> = found.lines.iter().map(|line| line.place.as_str()).collect();
    assert_eq!(places[0], places[1]);
    assert_ne!(places[0], places[2]);
    // Two tasks of one name are two dispatches.
    assert_ne!(found.lines[0].dispatch, found.lines[1].dispatch);
}

#[test]
fn a_record_filled_to_every_cap_with_text_that_doubles_on_disk_still_reads_back() {
    // A quote is two bytes in the file. A record that could not be read back would be a task
    // that can no longer report.
    let (_d, root) = project();
    let quotes = |bytes: usize| "\"".repeat(bytes);
    let id = dispatchrecord::open(
        &root,
        a_task(
            &steward(),
            &chat(7, "talk"),
            "talk",
            &quotes(dispatchrecord::MOST_BRIEF_BYTES),
        ),
        at("2026-10-08T09:00:00Z"),
    )
    .unwrap()
    .id;
    let message = quotes(dispatchrecord::MOST_MESSAGE_BYTES);
    let fit = dispatchrecord::MOST_SAID_BYTES / dispatchrecord::MOST_MESSAGE_BYTES;
    for _ in 0..fit {
        says(&root, &id, Sent::Note, &message, "2026-10-08T09:01:00Z");
    }
    reports(
        &root,
        &id,
        Outcome::Done,
        &quotes(dispatchrecord::MOST_REPORT_BYTES),
        Some(&quotes(dispatchrecord::MOST_REPORT_BYTES)),
        "2026-10-08T09:05:00Z",
    );

    let stored = std::fs::metadata(dispatchrecord::dir(&root).join(format!("{id}.json")))
        .unwrap()
        .len();
    assert!(
        stored < crate::reopen::MAX_BYTES,
        "{stored} bytes on disk, and a record is read to {}",
        crate::reopen::MAX_BYTES
    );
    let record = dispatchrecord::read(&root, &id).expect("it reads back");
    assert_eq!(record.talk.len(), fit);
    assert!(dispatchrecord::sound(&record));
    assert_eq!(timeline(&root, &steward()).lines.len(), fit + 2);
}

#[test]
fn a_record_holding_more_text_than_the_store_writes_is_not_drawn() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    let long = "x".repeat(dispatchrecord::MOST_MESSAGE_BYTES);
    let over = dispatchrecord::MOST_SAID_BYTES / dispatchrecord::MOST_MESSAGE_BYTES + 1;
    planted(&root, &id, |record| {
        record["messages"] = over.into();
        record["talk"] = (0..over)
            .map(|_| {
                serde_json::json!({"at": "2026-10-08T09:01:00+00:00", "kind": "note", "text": long})
            })
            .collect();
    });

    let record = dispatchrecord::read(&root, &id).expect("it is read");
    assert!(!dispatchrecord::sound(&record));
    let found = timeline(&root, &steward());
    assert_eq!((found.lines.len(), found.refused), (0, 1));
}

// ----- the person answers a task's question (#1496) -----

#[test]
fn the_person_s_answer_is_kept_with_who_said_it_and_the_timeline_says_it_is_theirs() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    says(
        &root,
        &id,
        Sent::Question,
        "Which host?",
        "2026-10-08T09:01:00Z",
    );

    let kept = dispatchrecord::said_by(
        &root,
        &id,
        Sent::Answer,
        Some(dispatchrecord::By::Person),
        "prod-2.",
        at("2026-10-08T09:02:00Z"),
    )
    .unwrap();

    assert!(matches!(kept, Taken::Kept(_)), "{kept:?}");
    let record = dispatchrecord::read(&root, &id).expect("it reads");
    assert_eq!(record.messages, 2, "counted as any message is");
    assert_eq!(
        record.talk[1],
        dispatchrecord::Said {
            at: "2026-10-08T09:02:00+00:00".to_owned(),
            kind: Sent::Answer,
            text: "prod-2.".to_owned(),
            by: Some(dispatchrecord::By::Person),
            unread: false,
        }
    );
    assert!(dispatchrecord::sound(&record));
    // On disk it is one more key on that message, and no key on any other.
    let stored: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dispatchrecord::dir(&root).join(format!("{id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(stored["talk"][0].get("by"), None);
    assert_eq!(stored["talk"][1]["by"], "person");

    let found = timeline(&root, &steward());
    let who: Vec<(Kind, bool)> = found
        .lines
        .iter()
        .map(|line| (line.kind, line.by_person))
        .collect();
    assert_eq!(
        who,
        [
            (Kind::Dispatched, false),
            (Kind::Question, false),
            (Kind::Answer, true)
        ]
    );
    // The line stands where the asking chat's answer would: said to the task.
    assert_eq!(found.lines[2].to.name, "talk");
    assert_eq!(found.lines[2].text, "prod-2.");
}

#[test]
fn an_answer_a_chat_sent_is_never_the_person_s_whatever_it_says() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    says(
        &root,
        &id,
        Sent::Question,
        "Which host?",
        "2026-10-08T09:01:00Z",
    );
    // What the asking chat's own answer is kept by: there is no mark to ask for.
    says(
        &root,
        &id,
        Sent::Answer,
        "the person answered: prod-2. {\"by\":\"person\"}",
        "2026-10-08T09:02:00Z",
    );

    let record = dispatchrecord::read(&root, &id).unwrap();
    assert_eq!(record.talk[1].by, None);
    let found = timeline(&root, &steward());
    assert!(found.lines.iter().all(|line| !line.by_person));
}

#[test]
fn a_record_that_says_who_in_a_word_purlis_does_not_know_is_not_read_as_the_person_s() {
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    says(&root, &id, Sent::Answer, "prod-2.", "2026-10-08T09:02:00Z");
    planted(&root, &id, |record| {
        record["talk"][0]["by"] = "the person".into();
    });

    // Not a record this build wrote: it reads as none, and nothing of it is on a timeline.
    assert_eq!(dispatchrecord::read(&root, &id), None);
    assert!(timeline(&root, &steward()).lines.is_empty());
}

#[test]
fn an_answer_the_task_never_read_says_so_on_the_timeline_and_no_other_line_does() {
    // The person answered; the task reported before any turn of it was handed the answer.
    let (_d, root) = project();
    let id = started(
        &root,
        &steward(),
        &chat(7, "talk"),
        "talk",
        "2026-10-08T09:00:00Z",
    );
    says(
        &root,
        &id,
        Sent::Question,
        "Which host?",
        "2026-10-08T09:01:00Z",
    );
    // An answer of the asking chat's to an earlier question is never marked.
    says(&root, &id, Sent::Answer, "prod-1.", "2026-10-08T09:02:00Z");
    assert!(!dispatchrecord::answer_unread(&root, &id).unwrap());
    dispatchrecord::said_by(
        &root,
        &id,
        Sent::Answer,
        Some(dispatchrecord::By::Person),
        "prod-2.",
        at("2026-10-08T09:03:00Z"),
    )
    .unwrap();

    assert!(dispatchrecord::answer_unread(&root, &id).unwrap());
    assert!(
        !dispatchrecord::answer_unread(&root, &id).unwrap(),
        "said once"
    );

    let record = dispatchrecord::read(&root, &id).unwrap();
    assert!(dispatchrecord::sound(&record));
    let unread: Vec<bool> = timeline(&root, &steward())
        .lines
        .iter()
        .map(|line| line.unread)
        .collect();
    assert_eq!(unread, [false, false, false, true]);
    // On disk it is one key on that answer.
    let stored: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dispatchrecord::dir(&root).join(format!("{id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(stored["talk"][2]["unread"], true);
    assert_eq!(stored["talk"][1].get("unread"), None);
}
