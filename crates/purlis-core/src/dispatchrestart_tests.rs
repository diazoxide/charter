use super::*;
use crate::dispatchrecord::{Asker, Opening, Worker};
use crate::harness::SessionId;
use crate::reopen::{HandedFrom, Identity};

const STEWARD: u32 = 3;
const TASK: u32 = 7;
const STEWARD_ID: &str = "01K6ASKER0000000000000000A";
const TASK_ID: &str = "01K6W0RKER000000000000000B";
const OTHER_ID: &str = "01K60THER0000000000000000C";
const BRIEF: &str = "# Check prod\nIs the rollout healthy?";

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

fn alpha() -> Place {
    Place::Workspace("alpha".to_owned())
}

/// The chat the person started, as the record of open chats has it.
fn the_steward() -> Chat {
    Chat {
        program: "claude".to_owned(),
        name: "3".to_owned(),
        persona: Some("steward".to_owned()),
        profile: Some("work".to_owned()),
        number: Some(STEWARD),
        resume: Some(SessionId::new("conv-steward").unwrap()),
        identity: Identity {
            id: Some(STEWARD_ID.to_owned()),
            ..Identity::default()
        },
        ..Chat::default()
    }
}

/// The task the steward chat dispatched, which still owes its report.
fn the_task() -> Chat {
    Chat {
        program: "claude".to_owned(),
        name: TASK.to_string(),
        label: Some("check prod".to_owned()),
        persona: Some("devops".to_owned()),
        profile: Some("work".to_owned()),
        number: Some(TASK),
        resume: Some(SessionId::new("conv-7").unwrap()),
        from: Some(HandedFrom {
            chat: STEWARD,
            name: "steward 3".to_owned(),
            workspace: alpha(),
            report: Owed::Due,
            mode: Mode::Task,
            depth: 1,
            root: Some(STEWARD_ID.to_owned()),
            by_person: false,
        }),
        identity: Identity {
            id: Some(TASK_ID.to_owned()),
            ..Identity::default()
        },
        ..Chat::default()
    }
}

fn recorded(chats: Vec<Chat>) -> reopen::Record {
    reopen::Record {
        chats,
        dealt: 20,
        ..reopen::Record::default()
    }
}

fn opening(brief: &str) -> Opening {
    Opening {
        mode: dispatchrecord::Mode::Task,
        asker: Asker {
            chat: ChatRef {
                chat: STEWARD,
                id: Some(STEWARD_ID.to_owned()),
                name: "steward 3".to_owned(),
                persona: Some("steward".to_owned()),
            },
            workspace: Some("alpha".to_owned()),
            by_person: false,
            session_record: None,
        },
        persona: Some("devops".to_owned()),
        worker: Worker {
            chat: ChatRef {
                chat: TASK,
                id: Some(TASK_ID.to_owned()),
                name: "check prod".to_owned(),
                persona: Some("devops".to_owned()),
            },
            harness: Some("claude".to_owned()),
            profile: Some("work".to_owned()),
            session_record: None,
        },
        task: Some("check prod".to_owned()),
        place: dispatchrecord::Place {
            workspace: Some("alpha".to_owned()),
            folder: Some("workspaces/alpha".to_owned()),
            worktree: None,
        },
        brief: brief.to_owned(),
        report_owed: true,
    }
}

/// The app's own record of the dispatch that started the task, still running.
fn dispatched(root: &Path) -> dispatchrecord::Record {
    dispatchrecord::open(root, opening(BRIEF), at("2026-10-09T08:00:00Z")).unwrap()
}

fn numbers(chats: &[Chat]) -> Vec<u32> {
    chats.iter().filter_map(|chat| chat.number).collect()
}

// ---- what a launch does with a task that had not reported -------------------------------------

#[test]
fn a_working_task_is_brought_back_under_its_session_and_told_to_carry_on() {
    let (_d, root) = project();
    dispatched(&root);
    let record = recorded(vec![the_steward(), the_task()]);

    let launch = at_launch(&record, &dispatchrecord::list(&root));

    // Both come back, and the task's entry is as it was: who asked, and what it owes.
    assert_eq!(launch.back, record);
    assert!(launch.not_resumed.is_empty());
    assert_eq!(launch.told(&the_task()), Some(CARRY_ON));
    // The chat the person started is told nothing: it resumes as it always did.
    assert_eq!(launch.told(&the_steward()), None);
}

#[test]
fn the_order_the_two_come_back_in_changes_nothing() {
    let (_d, root) = project();
    dispatched(&root);
    let running = dispatchrecord::list(&root);
    let asker_first = at_launch(&recorded(vec![the_steward(), the_task()]), &running);
    let task_first = at_launch(&recorded(vec![the_task(), the_steward()]), &running);

    assert_eq!(numbers(&asker_first.back.chats), [STEWARD, TASK]);
    assert_eq!(numbers(&task_first.back.chats), [TASK, STEWARD]);
    for launch in [&asker_first, &task_first] {
        assert_eq!(launch.told(&the_task()), Some(CARRY_ON));
        assert_eq!(launch.told(&the_steward()), None);
        assert!(launch.is_a_listed_task(TASK_ID));
    }
}

#[test]
fn what_a_restored_task_is_told_is_purlis_s_sentence_and_never_its_brief() {
    assert!(CARRY_ON.starts_with("purlis: "), "{CARRY_ON}");
    assert!(!CARRY_ON.contains("rollout"));
    // It is a harness's first message, so it is held to what one may be.
    assert_eq!(crate::handoff::bad_message(CARRY_ON), None);
    assert!(!CARRY_ON.contains('\n'));
}

#[test]
fn a_task_with_no_conversation_to_resume_is_left_out_and_not_started_on_its_brief() {
    let (_d, root) = project();
    dispatched(&root);
    let lost = Chat {
        resume: None,
        ..the_task()
    };
    let showing = Chat {
        shows: Some(TASK),
        ..the_steward()
    };

    let launch = at_launch(
        &recorded(vec![showing, lost.clone()]),
        &dispatchrecord::list(&root),
    );

    assert_eq!(numbers(&launch.back.chats), [STEWARD]);
    assert_eq!(launch.not_resumed, std::slice::from_ref(&lost));
    assert_eq!(launch.told(&lost), None);
    // The tab that showed it shows its own chat again.
    assert_eq!(launch.back.chats[0].shows, None);
}

#[test]
fn an_entry_the_dispatch_store_does_not_vouch_for_is_told_nothing() {
    let (_d, root) = project();
    // No dispatch record at all: the entry alone says it is a task.
    let record = recorded(vec![the_steward(), the_task()]);
    let launch = at_launch(&record, &dispatchrecord::list(&root));
    assert_eq!(launch.back, record);
    assert_eq!(launch.told(&the_task()), None);
    assert!(!launch.is_a_listed_task(TASK_ID));

    // A dispatch that has ended vouches for nothing either.
    let made = dispatched(&root);
    dispatchrecord::close(
        &root,
        &made.id,
        Ending::default(),
        at("2026-10-09T08:05:00Z"),
    )
    .unwrap();
    let launch = at_launch(&record, &dispatchrecord::list(&root));
    assert_eq!(launch.told(&the_task()), None);
}

#[test]
fn an_entry_that_names_another_chat_as_its_asker_is_told_nothing_and_never_reported_for() {
    let (_d, root) = project();
    dispatched(&root);
    // The entry was changed on disk to name chat 5 as the chat that asked. The dispatch
    // store says the steward chat asked, so nothing is acted on.
    let other = Chat {
        number: Some(5),
        identity: Identity {
            id: Some(OTHER_ID.to_owned()),
            ..Identity::default()
        },
        ..the_steward()
    };
    let mut bent = the_task();
    bent.from.as_mut().unwrap().chat = 5;
    let lost = Chat {
        resume: None,
        ..bent.clone()
    };

    let running = dispatchrecord::list(&root);
    let launch = at_launch(
        &recorded(vec![the_steward(), other.clone(), bent.clone()]),
        &running,
    );
    assert_eq!(launch.told(&bent), None);
    assert!(!launch.is_a_listed_task(TASK_ID));

    // With no conversation it comes back as the chat it was: nothing is reported to chat 5.
    let launch = at_launch(&recorded(vec![the_steward(), other, lost]), &running);
    assert!(launch.not_resumed.is_empty());
    assert_eq!(numbers(&launch.back.chats), [STEWARD, 5, TASK]);
}

#[test]
fn a_task_that_has_reported_or_was_handed_its_work_is_not_told_to_carry_on() {
    let (_d, root) = project();
    dispatched(&root);
    let running = dispatchrecord::list(&root);
    let mut reported = the_task();
    reported.from.as_mut().unwrap().report = Owed::Sent;
    let mut handoff = the_task();
    handoff.from.as_mut().unwrap().mode = Mode::Handoff;
    for chat in [reported, handoff] {
        let launch = at_launch(&recorded(vec![the_steward(), chat.clone()]), &running);
        assert_eq!(launch.told(&chat), None);
        assert!(launch.not_resumed.is_empty());
    }
}

#[test]
fn a_task_whose_asker_is_gone_carries_on_and_one_the_person_started_is_never_left_out() {
    let (_d, root) = project();
    dispatched(&root);
    let running = dispatchrecord::list(&root);
    // The chat that asked closed before the quit: the task is alone in the record.
    let launch = at_launch(&recorded(vec![the_task()]), &running);
    assert_eq!(launch.told(&the_task()), Some(CARRY_ON));
    assert!(!launch.is_a_listed_task(TASK_ID));
    // With no conversation it still comes back as the chat it was: nobody to list it under.
    let lost = Chat {
        resume: None,
        ..the_task()
    };
    let launch = at_launch(&recorded(vec![lost.clone()]), &running);
    assert_eq!(launch.back.chats, [lost]);

    // A task the person started from the steward chat's tab is their conversation.
    let mut theirs = the_task();
    theirs.from.as_mut().unwrap().by_person = true;
    theirs.resume = None;
    let launch = at_launch(&recorded(vec![the_steward(), theirs]), &running);
    assert!(launch.not_resumed.is_empty());
    assert!(!launch.is_a_listed_task(TASK_ID));
}

// ---- a task a launch could not bring back -----------------------------------------------------

#[test]
fn a_task_that_cannot_be_resumed_has_ended_by_itself_and_its_asker_is_told_once() {
    let (_d, root) = project();
    let made = dispatched(&root);
    let lost = Chat {
        resume: None,
        ..the_task()
    };

    let ended = end_at_launch(&root, &lost, true, None, at("2026-10-09T09:00:00Z"))
        .unwrap()
        .expect("it is settled");

    assert_eq!((ended.asker, ended.task), (STEWARD, Some(TASK)));
    assert_eq!(ended.name, "check prod");
    // Left for the asking chat's own next turn, in the words a task whose program ended is
    // reported for.
    let told = handback::take(&root, For::Chat(STEWARD));
    assert_eq!(told, std::slice::from_ref(&ended.report));
    let task = told[0].task.as_ref().expect("a task's report");
    assert!(task.unreported);
    assert_eq!(task.outcome, handback::Outcome::Failed);
    assert_eq!(told[0].summary, handback::UNREPORTED);
    assert_eq!(told[0].to, "steward 3");
    // Its dispatch record ended, in purlis's word, and says why.
    let record = dispatchrecord::read(&root, &made.id).unwrap();
    assert!(!record.running());
    assert_eq!(
        dispatchrecord::Finished::of(&record),
        Some(dispatchrecord::Finished::EndedWithoutAReport)
    );
    assert_eq!(
        record.report.as_ref().unwrap().text,
        "ended without a report: purlis was restarted, and its harness had named no \
         conversation to bring back"
    );
    assert_eq!(record.undelivered, None);
    assert!(dispatchrecord::sound(&record));

    // A second look at the same task tells nobody a second time.
    let again = end_at_launch(&root, &lost, true, None, at("2026-10-09T09:00:01Z")).unwrap();
    assert_eq!(again, None);
    assert!(handback::take(&root, For::Chat(STEWARD)).is_empty());
}

#[test]
fn a_task_whose_start_was_refused_keeps_its_conversation_for_a_reopen_and_says_why() {
    let (_d, root) = project();
    let made = dispatched(&root);

    let ended = end_at_launch(
        &root,
        &the_task(),
        true,
        Some("the profile 'work' is not declared on this machine"),
        at("2026-10-09T09:00:00Z"),
    )
    .unwrap();

    assert!(ended.is_some());
    let record = dispatchrecord::read(&root, &made.id).unwrap();
    assert_eq!(record.conversation.as_deref(), Some("conv-7"));
    assert_eq!(
        record.report.as_ref().unwrap().text,
        "ended without a report: purlis was restarted and could not start it again (the \
         profile 'work' is not declared on this machine)"
    );
}

#[test]
fn a_task_that_cannot_be_resumed_while_its_asker_is_not_open_is_kept_for_the_workspace() {
    let (_d, root) = project();
    let made = dispatched(&root);

    let ended = end_at_launch(&root, &the_task(), false, None, at("2026-10-09T09:00:00Z"))
        .unwrap()
        .expect("it is settled");

    assert_eq!(ended.file, None);
    assert!(handback::take(&root, For::Chat(STEWARD)).is_empty());
    let record = dispatchrecord::read(&root, &made.id).unwrap();
    let kept = record
        .undelivered
        .as_ref()
        .and_then(|kept| kept.kept.clone())
        .expect("the record says where it was kept");
    assert!(kept.starts_with("workspace-alpha/"), "{kept}");
    assert_eq!(handback::take(&root, For::Place(&alpha())), [ended.report]);
}

#[test]
fn nothing_is_reported_for_an_entry_with_no_running_dispatch_of_its_own() {
    let (_d, root) = project();
    // The store has no record of it: the entry's word alone ends nothing and tells nobody.
    let said = end_at_launch(&root, &the_task(), true, None, at("2026-10-09T09:00:00Z")).unwrap();
    assert_eq!(said, None);
    assert!(handback::take(&root, For::Chat(STEWARD)).is_empty());

    // And an entry with no id is matched to no dispatch by its number.
    dispatched(&root);
    let no_id = Chat {
        identity: Identity::default(),
        ..the_task()
    };
    let said = end_at_launch(&root, &no_id, true, None, at("2026-10-09T09:00:00Z")).unwrap();
    assert_eq!(said, None);
    assert!(dispatchrecord::list(&root)[0].running());
}

// ---- no brief twice ---------------------------------------------------------------------------

#[test]
fn the_brief_a_restored_task_is_working_on_is_not_dispatched_again() {
    let (_d, root) = project();
    let made = dispatched(&root);
    let restored = dispatchrecord::list(&root);

    let already = dispatched_before(&restored, Some("devops"), BRIEF).expect("it is running");
    assert_eq!(already.id, made.id);

    // Another brief, or the same one to another persona, is a new task.
    assert_eq!(
        dispatched_before(&restored, Some("devops"), "Check staging"),
        None
    );
    assert_eq!(dispatched_before(&restored, Some("qa"), BRIEF), None);
    assert_eq!(dispatched_before(&restored, None, BRIEF), None);
    let said = not_again("check prod", TASK);
    assert!(
        said.starts_with(
            "purlis did not dispatch this a second time: the same brief is already running as \
             'check prod' (chat 7)."
        ),
        "{said}"
    );
    assert!(said.contains(" wait 7`"), "{said}");
}

#[test]
fn a_restored_task_that_has_ended_no_longer_stands_in_a_brief_s_way() {
    let (_d, root) = project();
    let made = dispatched(&root);
    dispatchrecord::close(
        &root,
        &made.id,
        Ending::default(),
        at("2026-10-09T08:05:00Z"),
    )
    .unwrap();

    assert_eq!(
        dispatched_before(&dispatchrecord::list(&root), Some("devops"), BRIEF),
        None
    );
}

#[test]
fn a_long_brief_is_compared_as_the_store_kept_it() {
    let (_d, root) = project();
    let long = "x".repeat(dispatchrecord::MOST_BRIEF_BYTES + 4096);
    dispatchrecord::open(&root, opening(&long), at("2026-10-09T08:00:00Z")).unwrap();

    assert!(dispatched_before(&dispatchrecord::list(&root), Some("devops"), &long).is_some());
}

// ---- a report whose asking chat has gone ------------------------------------------------------

/// The task reported `done` after its asking chat had closed: kept for the workspace, and its
/// record says it reached no chat.
fn reported_to_nobody(root: &Path) -> (dispatchrecord::Record, Handback) {
    let made = dispatched(root);
    let report = Handback {
        from: "check prod".to_owned(),
        from_workspace: alpha(),
        to: "steward 3".to_owned(),
        to_workspace: alpha(),
        summary: "The rollout is healthy.".to_owned(),
        task: Some(handback::Task {
            outcome: handback::Outcome::Done,
            changed: None,
            record: None,
            by_person: false,
            unreported: false,
            stepped_in: false,
            branch: None,
        }),
        answered: None,
        stopped: None,
    };
    let kept = handback::leave_at(root, For::Place(&alpha()), &report).unwrap();
    let kept = handback::kept_name(root, &kept).expect("a kept report has a name");
    assert!(dispatchrecord::kept_undelivered(root, &made.id, Some(&kept)).unwrap());
    dispatchrecord::close(
        root,
        &made.id,
        Ending {
            report: Some(Report {
                outcome: Outcome::Done,
                text: report.summary.clone(),
                changed: dispatchrecord::Changed::default(),
            }),
            usage: None,
        },
        at("2026-10-09T08:30:00Z"),
    )
    .unwrap();
    (dispatchrecord::read(root, &made.id).unwrap(), report)
}

fn by_the_steward(asker: &Asker) -> bool {
    asker.chat.id.as_deref() == Some(STEWARD_ID)
}

#[test]
fn an_orphaned_task_s_report_is_kept_on_its_record_and_handed_to_its_asker_reopened() {
    let (_d, root) = project();
    let (record, report) = reported_to_nobody(&root);
    // Kept where the person finds it, whatever becomes of the copy for the workspace.
    assert_eq!(
        record.report.as_ref().unwrap().text,
        "The rollout is healthy."
    );
    assert!(record.undelivered.is_some());
    assert!(dispatchrecord::sound(&record));

    // The person reopens the chat that asked: taken back before it starts…
    let owing = take_back(&root, by_the_steward);
    assert_eq!(owing.len(), 1);
    assert_eq!(owing[0].report, report);
    assert!(
        handback::take(&root, For::Place(&alpha())).is_empty(),
        "the workspace's copy is gone, so the reopened chat's start does not read it too"
    );
    // …and left for its own next turn, under the number it has now.
    assert_eq!(hand_to(&root, &owing, 12), 1);
    assert_eq!(handback::take(&root, For::Chat(12)), [report]);
    assert_eq!(
        dispatchrecord::read(&root, &record.id).unwrap().undelivered,
        None
    );

    // Once: a second reopen is handed nothing.
    assert!(take_back(&root, by_the_steward).is_empty());
}

#[test]
fn a_report_another_chat_read_from_the_workspace_still_reaches_the_asker_reopened() {
    let (_d, root) = project();
    let (_record, report) = reported_to_nobody(&root);
    // A chat that started in the workspace meanwhile read the kept copy.
    assert_eq!(handback::take(&root, For::Place(&alpha())).len(), 1);

    let owing = take_back(&root, by_the_steward);

    assert_eq!(owing.len(), 1);
    assert_eq!(hand_to(&root, &owing, 12), 1);
    assert_eq!(handback::take(&root, For::Chat(12)), [report]);
}

#[test]
fn another_chat_reopened_is_handed_nothing_of_it() {
    let (_d, root) = project();
    let (record, _report) = reported_to_nobody(&root);

    let owing = take_back(&root, |asker| asker.chat.id.as_deref() == Some(OTHER_ID));

    assert!(owing.is_empty());
    assert_eq!(handback::take(&root, For::Place(&alpha())).len(), 1);
    assert!(
        dispatchrecord::read(&root, &record.id)
            .unwrap()
            .undelivered
            .is_some()
    );
}

#[test]
fn a_reopen_that_does_not_start_leaves_the_report_as_it_was() {
    let (_d, root) = project();
    let (record, report) = reported_to_nobody(&root);
    let owing = take_back(&root, by_the_steward);

    give_back(&root, &owing);

    let after = dispatchrecord::read(&root, &record.id).unwrap();
    let kept = after
        .undelivered
        .and_then(|kept| kept.kept)
        .expect("kept for the workspace again");
    assert!(kept.starts_with("workspace-alpha/"), "{kept}");
    // And the next reopen is handed it, once.
    let owing = take_back(&root, by_the_steward);
    assert_eq!(owing.len(), 1);
    assert_eq!(owing[0].report, report);
    assert!(handback::take(&root, For::Place(&alpha())).is_empty());
}

#[test]
fn a_kept_name_off_disk_that_is_not_one_purlis_gives_takes_nothing() {
    let (_d, root) = project();
    let (record, _report) = reported_to_nobody(&root);
    let good = record
        .undelivered
        .as_ref()
        .and_then(|kept| kept.kept.clone())
        .expect("a kept name");
    assert!(handback::a_kept_name(&good));
    // The same file's name in a chat's folder: that report is the chat's, never taken back.
    let a_chats = good.replacen("workspace-alpha/", "chat-3/", 1);
    for bad in [
        "../reopen.json",
        a_chats.as_str(),
        "workspace-alpha/../../x.json",
        "workspace-alpha/notakeptreport.json",
        "workspace-alpha",
    ] {
        assert!(!handback::a_kept_name(bad), "{bad}");
        assert!(!handback::withdraw(&root, bad), "{bad}");
    }
    // The real one is still there, untouched by any of them.
    assert_eq!(handback::take(&root, For::Place(&alpha())).len(), 1);
    // A record that names one is not one purlis draws.
    let mut bent = record.clone();
    bent.undelivered = Some(dispatchrecord::Undelivered {
        kept: Some("../reopen.json".to_owned()),
    });
    assert!(!dispatchrecord::sound(&bent));
}

#[test]
fn what_purlis_said_in_a_task_s_place_is_rebuilt_in_purlis_s_shape() {
    let (_d, root) = project();
    let made = dispatched(&root);
    end_at_launch(&root, &the_task(), false, None, at("2026-10-09T09:00:00Z")).unwrap();
    let record = dispatchrecord::read(&root, &made.id).unwrap();

    let told = report_of(&record).expect("a report");

    // The fixed sentence, never the record's longer text: a file that claims purlis's voice
    // in any other words is dropped when it is read.
    assert_eq!(told.summary, handback::UNREPORTED);
    assert!(told.task.as_ref().unwrap().unreported);
    handback::leave(&root, For::Chat(12), &told).unwrap();
    assert_eq!(handback::take(&root, For::Chat(12)), [told]);
}

#[test]
fn a_task_the_person_stopped_is_told_as_a_stop_and_one_still_running_as_nothing() {
    let (_d, root) = project();
    let made = dispatched(&root);
    assert_eq!(report_of(&made), None, "still running");
    dispatchrecord::close_by(
        &root,
        &made.id,
        Ending {
            report: Some(Report {
                outcome: Outcome::Stopped,
                text: handback::STOPPED.to_owned(),
                changed: dispatchrecord::Changed::default(),
            }),
            usage: None,
        },
        Some(EndedBy::Person),
        at("2026-10-09T08:30:00Z"),
    )
    .unwrap();

    let told = report_of(&dispatchrecord::read(&root, &made.id).unwrap()).expect("a word");

    assert!(told.task.is_none() && told.summary.is_empty());
    assert_eq!(
        told.stopped,
        Some(handback::Stopped {
            wrote: false,
            task: true,
            by_person: false,
            record: None,
        })
    );
    handback::leave(&root, For::Chat(12), &told).unwrap();
    assert_eq!(handback::take(&root, For::Chat(12)), [told]);
}

// ---- after a launch, and a chat closing with reports unread -----------------------------------

#[test]
fn a_dispatch_whose_chat_neither_came_back_nor_waits_ends_and_is_kept_for_its_asker() {
    let (_d, root) = project();
    let made = dispatched(&root);

    // The person chose to start fresh: nothing came back, and nothing waits.
    assert_eq!(
        settle_after_launch(&root, |_| false, at("2026-10-09T09:00:00Z")),
        1
    );

    let record = dispatchrecord::read(&root, &made.id).unwrap();
    assert_eq!(
        dispatchrecord::Finished::of(&record),
        Some(dispatchrecord::Finished::EndedWithoutAReport)
    );
    assert_eq!(
        record.undelivered,
        Some(dispatchrecord::Undelivered { kept: None })
    );
    // The chat that asked, brought back later, is told it ended without one.
    let owing = take_back(&root, by_the_steward);
    assert_eq!(owing.len(), 1);
    assert!(owing[0].report.task.as_ref().unwrap().unreported);
    assert_eq!(owing[0].asker.id.as_deref(), Some(STEWARD_ID));
}

#[test]
fn a_dispatch_whose_chat_came_back_is_left_running() {
    let (_d, root) = project();
    let made = dispatched(&root);

    let live = |worker: &ChatRef| worker.id.as_deref() == Some(TASK_ID);
    assert_eq!(
        settle_after_launch(&root, live, at("2026-10-09T09:00:00Z")),
        0
    );

    assert!(dispatchrecord::read(&root, &made.id).unwrap().running());
}

/// The task reported `done` to the steward chat's next turn, and its program was ended.
fn reported_and_left_for_the_steward(root: &Path) -> (dispatchrecord::Record, Handback) {
    let (record, report) = reported_to_nobody(root);
    // As `reported_to_nobody`, but the report was left for the chat and the record says nothing.
    handback::take(root, For::Place(&alpha()));
    dispatchrecord::delivered_late(root, &record.id).unwrap();
    handback::leave(root, For::Chat(STEWARD), &report).unwrap();
    (dispatchrecord::read(root, &record.id).unwrap(), report)
}

fn the_steward_ref() -> ChatRef {
    ChatRef {
        chat: STEWARD,
        id: Some(STEWARD_ID.to_owned()),
        name: "steward 3".to_owned(),
        persona: Some("steward".to_owned()),
    }
}

#[test]
fn a_report_its_asker_closed_without_reading_is_kept_for_it_should_it_be_reopened() {
    let (_d, root) = project();
    let (record, report) = reported_and_left_for_the_steward(&root);
    assert_eq!(record.undelivered, None);

    // The steward chat closes before its next turn: the report moves to its workspace.
    let moved = handback::orphan_kept(&root, STEWARD);
    assert_eq!(unread_at_close(&root, &the_steward_ref(), &moved), 1);

    let kept = dispatchrecord::read(&root, &record.id)
        .unwrap()
        .undelivered
        .and_then(|kept| kept.kept)
        .expect("named where it is kept");
    assert!(kept.starts_with("workspace-alpha/"), "{kept}");
    // Reopened, the steward chat is handed it, and the workspace is not handed it as well.
    let owing = take_back(&root, by_the_steward);
    assert_eq!(owing.len(), 1);
    assert_eq!(owing[0].report, report);
    assert!(handback::take(&root, For::Place(&alpha())).is_empty());
}

#[test]
fn a_report_that_is_no_task_s_marks_nothing_at_close() {
    let (_d, root) = project();
    let (record, _report) = reported_and_left_for_the_steward(&root);
    handback::take(&root, For::Chat(STEWARD));
    // A handoff's report, from a chat of the same name, says nothing of how the task ended.
    let handoffs = Handback {
        from: "check prod".to_owned(),
        from_workspace: alpha(),
        to: "steward 3".to_owned(),
        to_workspace: alpha(),
        summary: "The rollout is healthy.".to_owned(),
        task: None,
        answered: None,
        stopped: None,
    };
    handback::leave(&root, For::Chat(STEWARD), &handoffs).unwrap();

    let moved = handback::orphan_kept(&root, STEWARD);
    assert_eq!(unread_at_close(&root, &the_steward_ref(), &moved), 0);
    // And another chat's close marks nothing of the steward chat's tasks.
    handback::leave(&root, For::Chat(5), &report_of(&record).unwrap()).unwrap();
    let moved = handback::orphan_kept(&root, 5);
    let other = ChatRef {
        chat: 5,
        id: Some(OTHER_ID.to_owned()),
        ..the_steward_ref()
    };
    assert_eq!(unread_at_close(&root, &other, &moved), 0);
    assert_eq!(
        dispatchrecord::read(&root, &record.id).unwrap().undelivered,
        None
    );
}

#[test]
fn only_what_a_chat_dispatched_before_its_run_began_is_looked_among() {
    let (_d, root) = project();
    let minted_at = |time: &str| {
        ulid::Ulid::from_parts(u64::try_from(at(time).timestamp_millis()).unwrap(), 1).to_string()
    };
    let made = dispatchrecord::open_as(
        &root,
        minted_at("2026-10-09T08:00:00.500Z"),
        opening(BRIEF),
        at("2026-10-09T08:00:00Z"),
    )
    .unwrap();
    let steward = ChatRef {
        chat: STEWARD,
        id: Some(STEWARD_ID.to_owned()),
        ..ChatRef::default()
    };
    let other = ChatRef {
        id: Some(OTHER_ID.to_owned()),
        ..steward.clone()
    };
    let all = || dispatchrecord::list(&root);

    // A run that began after the dispatch: the chat was started again since it asked.
    let since = made_before_its_run(all(), &steward, &minted_at("2026-10-09T08:10:00Z"));
    assert_eq!(
        since.iter().map(|one| &one.id).collect::<Vec<_>>(),
        [&made.id]
    );
    // The run that dispatched it, begun in the same second and before it, looks among none.
    let same = made_before_its_run(all(), &steward, &minted_at("2026-10-09T08:00:00.100Z"));
    assert!(same.is_empty());
    // Nor another chat's run, nor one that is not a run's id.
    assert!(made_before_its_run(all(), &other, &minted_at("2026-10-09T08:10:00Z")).is_empty());
    assert!(made_before_its_run(all(), &steward, "not a run").is_empty());
}
