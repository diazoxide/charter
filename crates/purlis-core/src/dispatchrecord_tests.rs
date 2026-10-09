use super::*;

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

fn steward() -> ChatRef {
    ChatRef {
        chat: 3,
        id: Some("01K6ASKER0000000000000000A".to_owned()),
        name: "steward 3".to_owned(),
        persona: Some("steward".to_owned()),
    }
}

fn devops() -> ChatRef {
    ChatRef {
        chat: 7,
        id: Some("01K6W0RKER000000000000000B".to_owned()),
        name: "check prod".to_owned(),
        persona: Some("devops".to_owned()),
    }
}

/// A handoff from `steward 3` in `alpha` to `devops`, asking for a report.
fn a_handoff() -> Opening {
    Opening {
        mode: Mode::Handoff,
        asker: Asker {
            chat: steward(),
            workspace: Some("alpha".to_owned()),
            by_person: false,
            session_record: None,
        },
        persona: Some("devops".to_owned()),
        worker: Worker {
            chat: devops(),
            harness: Some("claude".to_owned()),
            profile: Some("work".to_owned()),
            session_record: None,
        },
        task: Some("check prod".to_owned()),
        place: Place {
            workspace: Some("beta".to_owned()),
            folder: Some("workspaces/beta".to_owned()),
            worktree: None,
        },
        brief: "# Check prod\nIs the rollout healthy?".to_owned(),
        report_owed: true,
    }
}

fn done(text: &str) -> Report {
    Report {
        outcome: Outcome::Done,
        text: text.to_owned(),
        changed: Changed {
            said: Some("values.yaml: replicas 2 to 3\nbranch fix/rollout, 1 commit".to_owned()),
            files: vec!["deploy/values.yaml".to_owned()],
            commits: vec!["3a823aab".to_owned()],
            branch: Some("fix/rollout".to_owned()),
        },
    }
}

#[test]
fn a_cancelled_task_is_recorded_as_cancelled_and_not_as_failed() {
    // D-1441-15, D-T59-j5: the record has a word of its own for a task its asking chat
    // cancelled, written as `cancelled` and read back so.
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let cancelled = Report {
        outcome: Outcome::Cancelled,
        text: "Stopped half way: two of five hosts checked.".to_owned(),
        changed: Changed::default(),
    };
    close(
        &root,
        &opened.id,
        Ending {
            report: Some(cancelled.clone()),
            usage: None,
        },
        at("2026-10-07T12:01:00Z"),
    )
    .unwrap();

    let record = read(&root, &opened.id).expect("it reads");
    assert_eq!(record.report, Some(cancelled));
    assert_eq!(Outcome::Cancelled.word(), "cancelled");
    // And one for a chat the person stopped (D-T59-j10), which did not fail by itself.
    assert_eq!(Outcome::Stopped.word(), "stopped");
    assert_eq!(
        serde_json::to_string(&Outcome::Stopped).unwrap(),
        r#""stopped""#
    );
    let text = std::fs::read_to_string(
        std::fs::read_dir(dir(&root))
            .unwrap()
            .flatten()
            .next()
            .expect("one record")
            .path(),
    )
    .unwrap();
    assert!(text.contains(r#""outcome": "cancelled""#), "{text}");
}

#[test]
fn a_finished_dispatch_s_record_holds_every_field() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    note(&root, &opened.id, Event::NeededYou).unwrap();
    note(&root, &opened.id, Event::NeededYou).unwrap();
    said(
        &root,
        &opened.id,
        crate::dispatchtalk::Kind::Note,
        "Two of three pods ready.",
        at("2026-10-07T12:02:00Z"),
    )
    .unwrap();

    let closed = close(
        &root,
        &opened.id,
        Ending {
            report: Some(done("Healthy: 3 of 3 pods ready.")),
            usage: Some(Usage {
                input_tokens: Some(15_234),
                output_tokens: Some(4_521),
                cost_usd: Some(0.42),
            }),
        },
        at("2026-10-07T12:04:30Z"),
    )
    .unwrap();

    assert!(closed);
    // Read back from the disk, as the app reads it after a restart.
    let records = list(&root);
    assert_eq!(records.len(), 1, "{records:?}");
    let record = &records[0];
    assert_eq!(record.id, opened.id);
    // Who asked: the chat and its persona.
    assert_eq!(record.asker.chat, steward());
    assert_eq!(record.asker.workspace.as_deref(), Some("alpha"));
    assert!(!record.asker.by_person);
    // Which persona it went to, and the chat that ran as it.
    assert_eq!(record.persona.as_deref(), Some("devops"));
    assert_eq!(record.worker.chat, devops());
    assert_eq!(record.worker.harness.as_deref(), Some("claude"));
    assert_eq!(record.mode, Mode::Handoff);
    // Where it worked.
    assert_eq!(record.place.workspace.as_deref(), Some("beta"));
    assert_eq!(record.place.folder.as_deref(), Some("workspaces/beta"));
    assert_eq!(record.task.as_deref(), Some("check prod"));
    assert_eq!(record.brief, "# Check prod\nIs the rollout healthy?");
    // The report: outcome, text, what changed.
    assert_eq!(record.report, Some(done("Healthy: 3 of 3 pods ready.")));
    assert_eq!(record.started, "2026-10-07T12:00:00+00:00");
    assert_eq!(record.ended.as_deref(), Some("2026-10-07T12:04:30+00:00"));
    assert_eq!(record.needed_you, 2);
    assert_eq!(record.messages, 1);
    assert_eq!(
        record.usage,
        Some(Usage {
            input_tokens: Some(15_234),
            output_tokens: Some(4_521),
            cost_usd: Some(0.42),
        })
    );
}

#[test]
fn cost_is_absent_and_not_zero_for_a_harness_that_reports_none() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();

    // A harness that says nothing, and one whose figure is empty: neither is a zero.
    close(
        &root,
        &opened.id,
        Ending {
            report: Some(done("ok")),
            usage: Some(Usage::default()),
        },
        at("2026-10-07T12:01:00Z"),
    )
    .unwrap();

    let record = read(&root, &opened.id).expect("the record");
    assert_eq!(record.usage, None);
    let text = std::fs::read_to_string(dir(&root).join(format!("{}.json", opened.id))).unwrap();
    assert!(!text.contains("usage"), "{text}");
    assert!(!text.contains("cost"), "{text}");
    assert!(!text.contains("tokens"), "{text}");
}

#[test]
fn a_harness_that_reports_only_tokens_has_no_cost() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();

    close(
        &root,
        &opened.id,
        Ending {
            report: None,
            usage: Some(Usage {
                input_tokens: Some(10),
                output_tokens: None,
                cost_usd: None,
            }),
        },
        at("2026-10-07T12:01:00Z"),
    )
    .unwrap();

    let usage = read(&root, &opened.id).unwrap().usage.expect("its tokens");
    assert_eq!(usage.input_tokens, Some(10));
    assert_eq!(usage.cost_usd, None);
}

#[test]
fn a_dispatch_ends_once_and_its_first_ending_is_kept() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let first = Ending {
        report: Some(done("first")),
        usage: None,
    };
    assert!(close(&root, &opened.id, first, at("2026-10-07T12:01:00Z")).unwrap());

    let again = Ending {
        report: Some(Report {
            outcome: Outcome::Failed,
            text: "second".to_owned(),
            changed: Changed::default(),
        }),
        usage: Some(Usage {
            cost_usd: Some(9.0),
            ..Usage::default()
        }),
    };
    assert!(!close(&root, &opened.id, again, at("2026-10-07T13:00:00Z")).unwrap());
    // And it counts nothing more.
    assert!(!note(&root, &opened.id, Event::NeededYou).unwrap());

    let record = read(&root, &opened.id).unwrap();
    assert_eq!(record.report, Some(done("first")));
    assert_eq!(record.ended.as_deref(), Some("2026-10-07T12:01:00+00:00"));
    assert_eq!(record.usage, None);
    assert_eq!(record.needed_you, 0);
}

#[test]
fn a_running_dispatch_has_no_end_and_no_report() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();

    let record = read(&root, &opened.id).unwrap();

    assert!(record.running());
    assert_eq!(record.ended, None);
    assert_eq!(record.report, None);
    assert_eq!(
        running_for(&root, &devops()).map(|found| found.id),
        Some(opened.id)
    );
    assert_eq!(running_for(&root, &steward()), None);
}

#[test]
fn records_are_listed_newest_first() {
    let (_d, root) = project();
    let first = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    let second = open(&root, a_handoff(), at("2026-10-07T12:00:01Z")).unwrap();

    let ids: Vec<String> = list(&root).into_iter().map(|record| record.id).collect();

    assert_eq!(ids, vec![second.id, first.id]);
}

#[test]
fn a_name_that_is_not_a_record_s_id_reads_and_writes_nothing() {
    let (_d, root) = project();
    open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let outside = root.join("outside.json");
    std::fs::write(&outside, "{}").unwrap();

    for id in ["../../outside", "", ".", "..", "x/y", "not-a-ulid"] {
        assert_eq!(read(&root, id), None, "{id}");
        assert!(!note(&root, id, Event::NeededYou).unwrap(), "{id}");
        assert!(
            !close(&root, id, Ending::default(), at("2026-10-07T12:01:00Z")).unwrap(),
            "{id}"
        );
    }
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "{}");
}

#[test]
fn a_file_that_is_not_a_record_of_this_version_is_skipped_and_left_as_it_is() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let newer = dir(&root).join("01K6NEWER00000000000000000.json");
    let said = "{\"v\": 2, \"id\": \"01K6NEWER00000000000000000\"}\n";
    std::fs::write(&newer, said).unwrap();
    std::fs::write(dir(&root).join("notes.txt"), "mine").unwrap();

    let ids: Vec<String> = list(&root).into_iter().map(|record| record.id).collect();

    assert_eq!(ids, vec![opened.id]);
    assert_eq!(std::fs::read_to_string(&newer).unwrap(), said);
}

#[cfg(unix)]
#[test]
fn a_record_is_private_to_the_person() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();

    let mode = std::fs::metadata(dir(&root).join(format!("{}.json", opened.id)))
        .unwrap()
        .permissions()
        .mode();

    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_record_lives_in_the_app_s_state_and_nowhere_git_carries() {
    let (_d, root) = project();
    open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();

    assert_eq!(
        dir(&root),
        crate::names::state(&root).join("app").join("dispatches")
    );
    // Nothing in the committed dispatch log.
    assert!(!crate::dispatch::dir(&root).exists());
}

#[test]
fn a_chat_that_went_without_reporting_is_settled_as_failed_where_it_owed_a_report() {
    let (_d, root) = project();
    let owed = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let not_owed = open(
        &root,
        Opening {
            report_owed: false,
            worker: Worker {
                chat: ChatRef {
                    chat: 8,
                    id: None,
                    name: "devops 8".to_owned(),
                    persona: Some("devops".to_owned()),
                },
                ..a_handoff().worker
            },
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();
    let still_running = open(
        &root,
        Opening {
            worker: Worker {
                chat: ChatRef {
                    chat: 9,
                    id: None,
                    name: "devops 9".to_owned(),
                    persona: Some("devops".to_owned()),
                },
                ..a_handoff().worker
            },
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();

    let ended = settle(&root, |chat| chat.chat == 9, at("2026-10-08T09:00:00Z"));

    assert_eq!(ended, 2);
    let owed = read(&root, &owed.id).unwrap();
    assert_eq!(
        owed.report,
        Some(Report {
            outcome: Outcome::Failed,
            text: "ended without a report".to_owned(),
            changed: Changed::default(),
        })
    );
    assert_eq!(owed.ended.as_deref(), Some("2026-10-08T09:00:00+00:00"));
    let not_owed = read(&root, &not_owed.id).unwrap();
    assert!(!not_owed.running());
    assert_eq!(not_owed.report, None);
    assert!(read(&root, &still_running.id).unwrap().running());
}

#[test]
fn opening_a_project_with_no_reopen_record_settles_every_running_dispatch() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();

    assert_eq!(settle_on_open(&root, at("2026-10-08T09:00:00Z")), 1);

    assert!(!read(&root, &opened.id).unwrap().running());
}

#[test]
fn a_reopen_record_that_cannot_be_read_settles_nothing() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let reopen = crate::reopen::path(&root);
    std::fs::create_dir_all(reopen.parent().unwrap()).unwrap();
    std::fs::write(&reopen, "{ not json").unwrap();

    assert_eq!(settle_on_open(&root, at("2026-10-08T09:00:00Z")), 0);

    assert!(read(&root, &opened.id).unwrap().running());
}

#[test]
fn a_session_record_lists_the_dispatches_its_chat_asked_for() {
    let (_d, root) = project();
    let first = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    let second = open(&root, a_handoff(), at("2026-10-07T12:10:00Z")).unwrap();
    let path = "workspaces/alpha/sessions/20261007-130000-rollout.md";

    assert_eq!(session_recorded(&root, &steward(), path), 2);
    // A dispatch it makes afterwards belongs to its next record.
    std::thread::sleep(std::time::Duration::from_millis(3));
    let later = open(&root, a_handoff(), at("2026-10-07T14:00:00Z")).unwrap();
    let next = "workspaces/alpha/sessions/20261007-150000-again.md";
    // Listed on it, and every dispatch the chat asked for knows it as its newest (#1513).
    assert_eq!(session_recorded(&root, &steward(), next), 3);
    for one in [&first, &second, &later] {
        assert_eq!(
            read(&root, &one.id).unwrap().asker_last_record.as_deref(),
            Some(next)
        );
    }

    let ids = |path: &str| -> Vec<String> {
        listed_on(&root, path)
            .into_iter()
            .map(|record| record.id)
            .collect()
    };
    // In the order they were made.
    assert_eq!(ids(path), vec![first.id, second.id]);
    assert_eq!(ids(next), vec![later.id]);
    assert_eq!(
        ids("workspaces/alpha/sessions/another.md"),
        Vec::<String>::new()
    );
}

#[test]
fn the_persona_chat_s_own_session_record_is_named_on_its_dispatch() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    close(
        &root,
        &opened.id,
        Ending::default(),
        at("2026-10-07T12:30:00Z"),
    )
    .unwrap();
    let path = "workspaces/beta/sessions/20261007-123000-check-prod.md";

    // After the dispatch ended: a record is written as its chat closes.
    assert_eq!(session_recorded(&root, &devops(), path), 1);

    let record = read(&root, &opened.id).unwrap();
    assert_eq!(record.worker.session_record.as_deref(), Some(path));
    assert_eq!(record.asker.session_record, None);
}

#[test]
fn a_restarted_chat_is_the_same_chat_by_its_id_whatever_its_number() {
    let restarted = ChatRef {
        chat: 12,
        ..devops()
    };
    assert!(same_chat(&devops(), &restarted));
}

/// A number is dealt again in another launch: a record that names its chat by id is never
/// another chat's because that chat has the number now, with an id of its own or with none.
#[test]
fn a_record_that_names_a_chat_by_id_is_never_matched_by_its_number() {
    let another = ChatRef {
        chat: 7,
        id: Some("01K6AN0THER000000000000000".to_owned()),
        ..devops()
    };
    assert!(!same_chat(&devops(), &another));
    let numbered = ChatRef {
        id: None,
        ..devops()
    };
    assert!(!same_chat(&devops(), &numbered));
    // Only a record with no id has nothing but the number to go by.
    assert!(same_chat(&numbered, &devops()));
    assert!(same_chat(&numbered, &another));
}

/// A reopen record holding one chat: number `number`, ULID `id`.
fn reopening(root: &Path, number: u32, id: &str) {
    let record = crate::reopen::Record {
        chats: vec![crate::reopen::Chat {
            program: "claude".into(),
            number: Some(number),
            identity: crate::reopen::Identity {
                id: Some(id.to_owned()),
                ..Default::default()
            },
            ..Default::default()
        }],
        dealt: number,
        ..Default::default()
    };
    crate::reopen::write(root, &record).unwrap();
}

#[test]
fn a_chat_started_again_under_another_number_is_not_settled_as_failed() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    // Restart gave the persona chat number 12; it is the same chat by its id.
    reopening(&root, 12, devops().id.as_deref().unwrap());

    assert_eq!(settle_on_open(&root, at("2026-10-08T09:00:00Z")), 0);

    let record = read(&root, &opened.id).unwrap();
    assert!(record.running());
    assert_eq!(record.report, None);
    // And the restarted chat's report still lands on it.
    let restarted = ChatRef {
        chat: 12,
        ..devops()
    };
    assert_eq!(
        running_for(&root, &restarted).map(|found| found.id),
        Some(opened.id)
    );
}

#[test]
fn a_chat_of_a_later_launch_that_was_dealt_the_same_number_does_not_keep_a_dispatch_running() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    // Another chat altogether is number 7 now.
    reopening(&root, 7, "01K6AN0THER000000000000000");

    assert_eq!(settle_on_open(&root, at("2026-10-08T09:00:00Z")), 1);

    let record = read(&root, &opened.id).unwrap();
    assert_eq!(
        record.report.map(|report| report.text),
        Some(ENDED_WITHOUT_A_REPORT.to_owned())
    );
    // Nor does that chat's session record land on the old dispatch.
    let namesake = ChatRef {
        chat: 7,
        id: Some("01K6AN0THER000000000000000".to_owned()),
        name: "qa 7".to_owned(),
        persona: Some("qa".to_owned()),
    };
    assert_eq!(session_recorded(&root, &namesake, "sessions/x.md"), 0);
    assert_eq!(running_for(&root, &namesake), None);
}

// ----- what is drawn, and what is not -----

/// A stored record with one thing changed, written as anything running as the person could
/// write it: straight to the file.
fn planted(root: &Path, change: impl FnOnce(&mut Record)) -> String {
    let mut record = open(root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    change(&mut record);
    std::fs::write(
        dir(root).join(format!("{}.json", record.id)),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    record.id
}

#[test]
fn a_record_the_app_wrote_is_drawn() {
    let (_d, root) = project();
    let opened = open(
        &root,
        Opening {
            brief: "# Check prod\n\tkubectl get pods\nEvery region.".to_owned(),
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();
    close(
        &root,
        &opened.id,
        Ending {
            report: Some(done("Healthy.\nNothing to do.")),
            usage: None,
        },
        at("2026-10-07T12:01:00Z"),
    )
    .unwrap();

    let drawn = drawn(&root);

    assert_eq!(drawn.refused, 0);
    assert_eq!(drawn.records.len(), 1);
}

#[test]
fn a_record_holding_text_purlis_will_not_draw_is_counted_and_never_shown() {
    let (_d, root) = project();
    let good = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    let bad = [
        // Words turned backwards: a task that reads as another.
        planted(&root, |record| {
            record.task = Some("check \u{202e}dorp".to_owned());
        }),
        // An invisible character: a persona that looks like another.
        planted(&root, |record| {
            record.persona = Some("dev\u{200b}ops".to_owned());
        }),
        planted(&root, |record| {
            record.asker.chat.name = "steward\u{2066} 3".to_owned();
        }),
        // A control character in a line, and one in prose that is not a line break or a tab.
        planted(&root, |record| {
            record.worker.chat.name = "devops\n7".to_owned();
        }),
        planted(&root, |record| record.brief = "a brief\u{1b}[2J".to_owned()),
        planted(&root, |record| {
            record.report = Some(Report {
                outcome: Outcome::Done,
                text: "fine\u{feff}".to_owned(),
                changed: Changed::default(),
            });
        }),
        planted(&root, |record| {
            record.place.folder = Some("workspaces/\u{202e}ateb".to_owned());
        }),
        // Text past what the store ever writes.
        planted(&root, |record| {
            record.brief = "x".repeat(MOST_BRIEF_BYTES * 2)
        }),
        planted(&root, |record| {
            record.task = Some("y".repeat(MOST_NAME_BYTES * 2));
        }),
        planted(&root, |record| {
            record.report = Some(Report {
                outcome: Outcome::Done,
                text: "ok".to_owned(),
                changed: Changed {
                    files: vec!["f".to_owned(); MOST_LISTED + 1],
                    ..Changed::default()
                },
            });
        }),
        // What a report says changed is a chat's words too, and held to the same rule.
        planted(&root, |record| {
            record.report = Some(Report {
                outcome: Outcome::Done,
                text: "ok".to_owned(),
                changed: Changed {
                    said: Some("svc: 2 files\u{202e}".to_owned()),
                    ..Changed::default()
                },
            });
        }),
    ];

    let drawn = drawn(&root);

    assert_eq!(drawn.refused, bad.len());
    let shown: Vec<&str> = drawn
        .records
        .iter()
        .map(|record| record.id.as_str())
        .collect();
    assert_eq!(shown, [good.id.as_str()]);
    // Refused, never stripped: each is on the disk as it was, for a person to look at.
    for id in &bad {
        assert!(read(&root, id).is_some(), "{id}");
    }
    // And none is listed on a session record.
    session_recorded(&root, &steward(), "sessions/x.md");
    assert_eq!(listed_on(&root, "sessions/x.md").len(), 1);
}

#[test]
fn a_brief_or_a_report_longer_than_the_store_keeps_is_cut_and_says_so() {
    let (_d, root) = project();
    let long = format!("{}é", "b".repeat(MOST_BRIEF_BYTES * 80));
    let opened = open(
        &root,
        Opening {
            brief: long,
            task: Some("t".repeat(MOST_NAME_BYTES * 4)),
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();

    // The close is never refused for its size.
    let closed = close(
        &root,
        &opened.id,
        Ending {
            report: Some(Report {
                outcome: Outcome::Done,
                text: "r".repeat(MOST_REPORT_BYTES * 200),
                changed: Changed {
                    said: Some("s".repeat(MOST_REPORT_BYTES * 20)),
                    files: vec!["f".repeat(MOST_PATH_BYTES * 3); MOST_LISTED * 5],
                    commits: vec!["c".repeat(64); MOST_LISTED * 5],
                    branch: None,
                },
            }),
            usage: None,
        },
        at("2026-10-07T12:01:00Z"),
    )
    .unwrap();

    assert!(closed);
    let size = std::fs::metadata(dir(&root).join(format!("{}.json", opened.id)))
        .unwrap()
        .len();
    assert!(size < crate::reopen::MAX_BYTES, "{size} bytes");
    // It reads back, whole as it was stored, and it is drawn.
    let drawn = drawn(&root);
    assert_eq!(drawn.refused, 0);
    let record = &drawn.records[0];
    assert!(
        record
            .brief
            .ends_with(&format!(" [cut at {MOST_BRIEF_BYTES} bytes]")),
        "{}",
        &record.brief[record.brief.len() - 60..]
    );
    assert!(record.brief.starts_with("bbbb"));
    let report = record.report.as_ref().unwrap();
    assert!(
        report
            .text
            .ends_with(&format!(" [cut at {MOST_REPORT_BYTES} bytes]"))
    );
    assert_eq!(report.changed.files.len(), MOST_LISTED);
    assert_eq!(report.changed.commits.len(), MOST_LISTED);
    assert!(
        record
            .task
            .as_deref()
            .unwrap()
            .ends_with(&format!(" [cut at {MOST_NAME_BYTES} bytes]"))
    );
    // A text already cut is not cut again by the next write.
    note(&root, &opened.id, Event::NeededYou).unwrap();
    assert_eq!(read(&root, &opened.id).unwrap().brief, record.brief);
}

#[test]
fn a_text_is_cut_at_a_character_and_one_that_fits_is_left_as_it_is() {
    assert_eq!(cut("short", 4096), "short");
    // Two bytes a letter: the cut falls back to the letter's start.
    let cut_text = cut(&"é".repeat(200), 101);
    assert!(cut_text.starts_with(&"é".repeat(50)));
    assert_eq!(cut_text, format!("{} [cut at 101 bytes]", "é".repeat(50)));
}

/// **A forged line.** The asks a chat sends the app are a closed set of shapes
/// ([`crate::hookwire::Ask`]), and none has a field for a dispatch's record, its asker, its
/// outcome, its counts or its cost. So a line forged with them either does not read as an ask
/// at all, or reads as the ask it would have been without them: nothing of a record crosses
/// the wire for the app to be misled by.
#[test]
fn a_line_forged_with_a_record_s_fields_carries_none_of_them_past_the_wire() {
    use crate::hookwire::{Ask, OpenChat, RecordAsk, ReportBack};

    let asks = [
        Ask::Ticket { chat: 7 },
        Ask::Report(Box::new(ReportBack {
            chat: 7,
            summary: "done".to_owned(),
            ticket: "t".to_owned(),
            task: None,
        })),
        Ask::Open(Box::new(OpenChat {
            chat: 3,
            workspace: "alpha".to_owned(),
            create_vision: None,
            persona: Some("devops".to_owned()),
            message: "a brief".to_owned(),
            ticket: "t".to_owned(),
            name: None,
            report: true,
        })),
        Ask::SessionRecord(Box::new(RecordAsk {
            chat: 7,
            title: "t".to_owned(),
            body: "b".to_owned(),
            pieces: Vec::new(),
            cwd: None,
        })),
    ];
    let forged = serde_json::json!({
        "id": "01K6FORGED0000000000000000",
        "record": "01K6FORGED0000000000000000",
        "dispatch": "01K6FORGED0000000000000000",
        "asker": {"chat": 99, "name": "someone else"},
        "by_person": true,
        "worker": {"chat": 99},
        "outcome": "failed",
        "needed_you": 40,
        "messages": 40,
        "usage": {"cost_usd": 1000.0, "input_tokens": 1},
        "cost_usd": 0.0,
        "ended": "2020-01-01T00:00:00+00:00",
    });
    for ask in asks {
        let mut line = serde_json::to_value(&ask).unwrap();
        // Into the ask's own object, and beside it.
        let inner = line
            .as_object_mut()
            .and_then(|object| object.values_mut().next())
            .and_then(serde_json::Value::as_object_mut)
            .expect("an ask is one key naming an object");
        for (key, value) in forged.as_object().unwrap() {
            inner.entry(key.clone()).or_insert_with(|| value.clone());
        }
        // Read as the ask it was, every forged key fallen off; or not an ask at all.
        if let Ok(read) = serde_json::from_value::<Ask>(line) {
            assert_eq!(read, ask);
        }
    }
}

// ----- a worktree of its own (#1453) ------------------------------------------------------

/// A task that was given a worktree of `api` in `alpha`, named for the dispatch.
fn a_worktree_task(piece: &str) -> Opening {
    Opening {
        mode: Mode::Task,
        place: Place {
            workspace: Some("alpha".to_owned()),
            folder: Some(format!("workspaces/alpha/.worktrees/api/{piece}")),
            worktree: Some(Worktree {
                repo: "api".to_owned(),
                piece: piece.to_owned(),
                branch: Some(piece.to_owned()),
                removed: None,
            }),
        },
        ..a_handoff()
    }
}

#[test]
fn a_record_is_opened_under_the_id_its_worktree_was_named_for_and_only_once() {
    let (_d, root) = project();
    let id = mint();
    let piece = crate::dispatchplace::piece_name("check prod", &id);

    let opened = open_as(
        &root,
        id.clone(),
        a_worktree_task(&piece),
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();

    assert_eq!(opened.id, id);
    let read = read(&root, &id).expect("the record");
    let tree = read.place.worktree.as_ref().expect("its worktree");
    assert_eq!(tree.repo, "api");
    assert_eq!(tree.branch.as_deref(), Some(piece.as_str()));
    assert_eq!(tree.removed, None);
    assert!(
        piece.ends_with(&id[id.len() - 8..].to_lowercase()),
        "{piece} is named for {id}"
    );
    // A second record under that id is refused, and the first stays as it was.
    let again = open_as(&root, id.clone(), a_handoff(), at("2026-10-07T12:01:00Z"));
    assert_eq!(
        again.expect_err("written once").kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(super::read(&root, &id), Some(read));
    // And an id purlis did not mint names no record.
    for forged in ["../../x", "", "not-a-ulid", "01k6z3v9qj8m4t2w7xb5rc0def"] {
        let refused = open_as(
            &root,
            forged.to_owned(),
            a_handoff(),
            at("2026-10-07T12:00:00Z"),
        );
        assert!(refused.is_err(), "{forged:?}");
    }
    assert_eq!(list(&root).len(), 1);
}

#[test]
fn how_a_worktree_went_is_recorded_once_and_never_for_a_dispatch_still_running() {
    let (_d, root) = project();
    let opened = open(
        &root,
        a_worktree_task("check-prod-b5rc0def"),
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();
    let removed = |root: &Path| {
        read(root, &opened.id)
            .and_then(|record| record.place.worktree)
            .and_then(|tree| tree.removed)
    };

    // Its chat still works there.
    assert!(!worktree_removed(&root, &opened.id, Removed::Merged).unwrap());
    assert_eq!(removed(&root), None);

    close(
        &root,
        &opened.id,
        Ending::default(),
        at("2026-10-07T12:05:00Z"),
    )
    .unwrap();
    assert!(worktree_removed(&root, &opened.id, Removed::Merged).unwrap());
    assert_eq!(removed(&root), Some(Removed::Merged));
    // The first word is the one kept.
    assert!(!worktree_removed(&root, &opened.id, Removed::Discarded).unwrap());
    assert_eq!(removed(&root), Some(Removed::Merged));
    // Written as one word, and drawn.
    let text = std::fs::read_to_string(dir(&root).join(format!("{}.json", opened.id))).unwrap();
    assert!(text.contains("\"removed\": \"merged\""), "{text}");
    assert_eq!(drawn(&root).refused, 0);

    // A dispatch that had no worktree has nothing to mark.
    let plain = open(&root, a_handoff(), at("2026-10-07T12:10:00Z")).unwrap();
    close(
        &root,
        &plain.id,
        Ending::default(),
        at("2026-10-07T12:11:00Z"),
    )
    .unwrap();
    assert!(!worktree_removed(&root, &plain.id, Removed::Discarded).unwrap());
}

#[test]
fn the_newest_dispatch_a_chat_worked_on_is_found_running_or_ended() {
    let (_d, root) = project();
    assert_eq!(latest_for(&root, &devops()), None);
    let first = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    close(
        &root,
        &first.id,
        Ending::default(),
        at("2026-10-07T12:01:00Z"),
    )
    .unwrap();

    assert_eq!(
        latest_for(&root, &devops()).map(|record| record.id),
        Some(first.id)
    );
    // The chat that asked is not the chat that worked.
    assert_eq!(latest_for(&root, &steward()), None);
}

// ----- what the report says changed, and the count `persona stats` reads (#1452) -----

#[test]
fn what_a_report_says_changed_is_kept_as_said_and_is_absent_where_it_said_nothing() {
    let (_d, root) = project();
    let told = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let silent = open(&root, a_handoff(), at("2026-10-07T12:00:01Z")).unwrap();
    let said = "svc: 2 files\nbranch fix/queue, 1 commit";
    for (id, changed) in [
        (
            &told.id,
            Changed {
                said: Some(said.to_owned()),
                ..Changed::default()
            },
        ),
        (&silent.id, Changed::default()),
    ] {
        let report = Report {
            outcome: Outcome::Done,
            text: "Drained.".to_owned(),
            changed,
        };
        let ending = Ending {
            report: Some(report),
            usage: None,
        };
        assert!(close(&root, id, ending, at("2026-10-07T12:05:00Z")).unwrap());
    }

    let kept = |id: &str| read(&root, id).and_then(|record| record.report).unwrap();
    assert_eq!(kept(&told.id).changed.said.as_deref(), Some(said));
    assert_eq!(kept(&silent.id).changed, Changed::default());
    let on_disk =
        |id: &str| std::fs::read_to_string(dir(&root).join(format!("{id}.json"))).unwrap();
    assert!(on_disk(&told.id).contains(r#""said": "svc: 2 files\nbranch fix/queue, 1 commit""#));
    assert!(
        !on_disk(&silent.id).contains("said"),
        "{}",
        on_disk(&silent.id)
    );
    // A record written before the field reads as one that said nothing.
    let before = r#"{"outcome":"done","text":"ok","changed":{"branch":"b"}}"#;
    let read: Report = serde_json::from_str(before).unwrap();
    assert_eq!(read.changed.said, None);
    assert_eq!(drawn(&root).refused, 0);
}

#[test]
fn the_tally_counts_every_dispatch_a_persona_was_given_running_or_ended() {
    let (_d, root) = project();
    // No store yet: nothing is counted, and that is an answer.
    assert_eq!(tally(&root).unwrap(), std::collections::BTreeMap::new());

    let to = |persona: Option<&str>, mode: Mode| Opening {
        persona: persona.map(str::to_owned),
        mode,
        ..a_handoff()
    };
    let first = open(
        &root,
        to(Some("devops"), Mode::Task),
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();
    open(
        &root,
        to(Some("devops"), Mode::Handoff),
        at("2026-10-07T12:00:01Z"),
    )
    .unwrap();
    open(
        &root,
        to(Some("qa"), Mode::Task),
        at("2026-10-07T12:00:02Z"),
    )
    .unwrap();
    // A chat started as no persona is nobody's dispatch.
    open(&root, to(None, Mode::Handoff), at("2026-10-07T12:00:03Z")).unwrap();
    close(
        &root,
        &first.id,
        Ending::default(),
        at("2026-10-07T12:09:00Z"),
    )
    .unwrap();
    // What is not a record is not counted: a note, and a write that was cut short.
    std::fs::write(dir(&root).join("notes.json"), "{}").unwrap();
    std::fs::write(
        dir(&root).join(format!(".purlis-generated.{}.json.41.7.tmp", first.id)),
        std::fs::read(dir(&root).join(format!("{}.json", first.id))).unwrap(),
    )
    .unwrap();

    let counted = tally(&root).unwrap();

    assert_eq!(
        counted.into_iter().collect::<Vec<_>>(),
        [("devops".to_owned(), 2), ("qa".to_owned(), 1)]
    );
}

#[cfg(unix)]
#[test]
fn a_store_that_cannot_be_read_is_an_error_and_never_a_count_of_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, root) = project();
    open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let store = dir(&root);
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads through any mode: there the refusal cannot be made, and nothing is claimed.
    let refused = std::fs::read_dir(&store).is_err();

    let counted = tally(&root);

    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).unwrap();
    if refused {
        assert!(counted.is_err(), "{counted:?}");
    }
}

#[test]
fn only_a_record_s_own_cut_short_write_is_named_as_its_temporary_file() {
    let id = "01K6D5PATCH000000000000000";
    assert!(an_id(id));
    for name in [
        format!(".purlis-generated.{id}.json.4171.9f3a.tmp"),
        format!(".charter-generated.{id}.json.1.0.tmp"),
    ] {
        assert!(a_record_s_temp(&name), "{name}");
        assert!(!a_record(&name), "{name}");
    }
    for name in [
        format!("{id}.json"),
        format!("{id}.json.tmp"),
        format!(".purlis-generated.{id}.json.tmp"),
        format!(".purlis-generated.{id}.json.4171.tmp"),
        ".purlis-generated.notes.json.4171.9f3a.tmp".to_owned(),
        format!(".purlis-generated.{id}.json.4171.9f3a"),
        format!(".other.{id}.json.4171.9f3a.tmp"),
    ] {
        assert!(!a_record_s_temp(&name), "{name}");
    }
}

// ----- a finished task's row (#1485) ---------------------------------------------------------

/// `steward 3` dispatched `name` as a task, and it ended with `report`.
fn a_finished_task(root: &Path, name: &str, worker: u32, report: Report, minute: u32) -> Record {
    let opening = Opening {
        mode: Mode::Task,
        task: Some(name.to_owned()),
        worker: Worker {
            chat: ChatRef {
                chat: worker,
                id: Some(mint()),
                name: name.to_owned(),
                persona: Some("devops".to_owned()),
            },
            ..a_handoff().worker
        },
        ..a_handoff()
    };
    let opened = open(root, opening, at(&format!("2026-10-07T12:{minute:02}:00Z"))).unwrap();
    close(
        root,
        &opened.id,
        Ending {
            report: Some(report),
            usage: None,
        },
        at(&format!("2026-10-07T12:{minute:02}:30Z")),
    )
    .unwrap();
    read(root, &opened.id).unwrap()
}

fn ended(outcome: Outcome, text: &str) -> Report {
    Report {
        outcome,
        text: text.to_owned(),
        changed: Changed::default(),
    }
}

fn nobody_open(_: &ChatRef) -> bool {
    false
}

#[test]
fn a_finished_task_is_listed_under_the_chat_that_asked_until_it_is_cleared() {
    let (_d, root) = project();
    let first = a_finished_task(&root, "check prod", 7, done("Healthy."), 1);
    let second = a_finished_task(&root, "check staging", 8, done("Healthy too."), 2);
    // A handoff that ended is not a task, and a task still running has not finished.
    let moved = open(&root, a_handoff(), at("2026-10-07T12:03:00Z")).unwrap();
    close(
        &root,
        &moved.id,
        Ending {
            report: Some(done("Moved.")),
            usage: None,
        },
        at("2026-10-07T12:04:00Z"),
    )
    .unwrap();
    open(
        &root,
        Opening {
            mode: Mode::Task,
            ..a_handoff()
        },
        at("2026-10-07T12:05:00Z"),
    )
    .unwrap();

    // Oldest first, read from the store: what an app started again reads too.
    let listed = |root: &Path| -> Vec<String> {
        finished_for(root, &steward(), nobody_open)
            .into_iter()
            .map(|record| record.id)
            .collect()
    };
    assert_eq!(listed(&root), vec![first.id.clone(), second.id.clone()]);
    // Another chat's list has none of them, whatever number it has.
    let other = ChatRef {
        id: Some("01K6SOMEONEELSE0000000000C".to_owned()),
        ..steward()
    };
    assert!(finished_for(&root, &other, nobody_open).is_empty());

    // Clearing one takes its row and nothing else: the record reads as it did, but for the
    // mark, and a second clear changes nothing.
    assert!(clear(&root, &first.id).unwrap());
    assert!(!clear(&root, &first.id).unwrap());
    assert_eq!(listed(&root), vec![second.id.clone()]);
    let kept = read(&root, &first.id).expect("the record stays");
    assert_eq!(
        kept,
        Record {
            cleared: true,
            ..first.clone()
        }
    );
    // A dispatch that has not finished has no row to clear.
    assert!(!clear(&root, &moved.id).unwrap());
}

#[test]
fn a_task_whose_chat_is_still_open_is_not_listed_as_finished_yet() {
    // Its report is in and its record has ended, and purlis has not ended its program yet:
    // it is still drawn as the open chat it is, once.
    let (_d, root) = project();
    let task = a_finished_task(&root, "check prod", 7, done("Healthy."), 1);
    let its_chat = task.worker.chat.clone();
    assert!(finished_for(&root, &steward(), |worker| same_chat(worker, &its_chat)).is_empty());
    assert_eq!(finished_for(&root, &steward(), nobody_open).len(), 1);
}

#[test]
fn the_rows_of_a_chat_s_finished_tasks_go_when_it_closes_and_their_records_stay() {
    let (_d, root) = project();
    let done_one = a_finished_task(&root, "check prod", 7, done("Healthy."), 1);
    let failed = a_finished_task(
        &root,
        "check staging",
        8,
        ended(Outcome::Failed, "The cluster refused the login."),
        2,
    );

    assert_eq!(clear_for(&root, &steward()), 2);
    assert!(finished_for(&root, &steward(), nobody_open).is_empty());
    assert_eq!(clear_for(&root, &steward()), 0);
    for id in [&done_one.id, &failed.id] {
        let kept = read(&root, id).expect("the record stays");
        assert!(kept.cleared);
        assert!(kept.report.is_some(), "the report is still readable");
    }
    assert_eq!(list(&root).len(), 2);
}

#[test]
fn done_and_cancelled_fold_and_every_other_end_stays_a_row_of_its_own() {
    // V100-9: a failure is never hidden behind a count.
    let (_d, root) = project();
    let said = |outcome: Outcome, text: &str| {
        let record = a_finished_task(&root, "a task", 7, ended(outcome, text), 1);
        let how = Finished::of(&record).expect("a finished task");
        (how.word(), how.folds())
    };
    assert_eq!(said(Outcome::Done, "All good."), ("done", true));
    assert_eq!(
        said(Outcome::Cancelled, "Stopped half way."),
        ("cancelled", true)
    );
    assert_eq!(said(Outcome::Failed, "It broke."), ("failed", false));
    assert_eq!(said(Outcome::Blocked, "Needs a login."), ("blocked", false));
    // **A chat's words never pass for purlis's**: a task that reports `failed` with purlis's
    // own sentence is a failed task, and says so.
    assert_eq!(
        said(Outcome::Failed, ENDED_WITHOUT_A_REPORT),
        ("failed", false)
    );
    assert_eq!(
        said(Outcome::Failed, crate::handback::UNREPORTED),
        ("failed", false)
    );
    // A record from before the way was kept: the person ended it, and it is read as closed.
    assert_eq!(
        said(Outcome::Stopped, crate::handback::STOPPED),
        ("closed by you", false)
    );
    // Who ended it is the app's fact, written with the ending.
    let ended_by = |outcome: Outcome, text: &str, by: EndedBy| {
        let opened = open(
            &root,
            Opening {
                mode: Mode::Task,
                ..a_handoff()
            },
            at("2026-10-07T12:10:00Z"),
        )
        .unwrap();
        close_by(
            &root,
            &opened.id,
            Ending {
                report: Some(ended(outcome, text)),
                usage: None,
            },
            Some(by),
            at("2026-10-07T12:11:00Z"),
        )
        .unwrap();
        let record = read(&root, &opened.id).unwrap();
        assert_eq!(record.ended_by, Some(by));
        let how = Finished::of(&record).unwrap();
        (how.word(), how.folds())
    };
    assert_eq!(
        ended_by(Outcome::Failed, "anything at all", EndedBy::Unreported),
        ("ended without a report", false)
    );
    // A task the person stopped does not fold, whatever its own last report says.
    assert_eq!(
        ended_by(Outcome::Done, "All good, honest.", EndedBy::Person),
        ("closed by you", false)
    );
    assert_eq!(
        serde_json::to_string(&EndedBy::Person).unwrap(),
        r#""person""#
    );
    // A handoff's record is no finished task, whatever it ended with.
    let moved = open(&root, a_handoff(), at("2026-10-07T12:03:00Z")).unwrap();
    assert_eq!(Finished::of(&moved), None);
}

#[test]
fn the_conversation_a_task_ended_in_is_kept_for_a_reopen() {
    let (_d, root) = project();
    let running = open(
        &root,
        Opening {
            mode: Mode::Task,
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();
    // Not while it runs: its chat may still move to another conversation.
    assert!(!ended_in(&root, &running.id, "9f2c-the-conversation").unwrap());

    let task = a_finished_task(&root, "check prod", 7, done("Healthy."), 1);
    assert_eq!(task.conversation, None);
    assert!(ended_in(&root, &task.id, "9f2c-the-conversation").unwrap());
    assert!(!ended_in(&root, &task.id, "9f2c-the-conversation").unwrap());
    let kept = read(&root, &task.id).unwrap();
    assert_eq!(kept.conversation.as_deref(), Some("9f2c-the-conversation"));
    assert!(sound(&kept));
    // And a record written before this field reads with none.
    let text = serde_json::to_string(&task).unwrap();
    assert!(!text.contains("conversation"), "{text}");
    assert!(!text.contains("cleared"), "{text}");

    // The task a chat was reopened from is found by the id its persona chat had.
    let worker = task.worker.chat.id.clone().unwrap();
    assert_eq!(
        task_worked_by(&root, &worker).map(|record| record.id),
        Some(task.id)
    );
    assert_eq!(task_worked_by(&root, "01K6NOBODY0000000000000000"), None);
}

#[test]
fn a_dispatch_settled_as_gone_without_a_report_is_said_so_by_the_app_s_own_mark() {
    let (_d, root) = project();
    let opened = open(
        &root,
        Opening {
            mode: Mode::Task,
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();

    assert_eq!(settle(&root, |_| false, at("2026-10-07T12:30:00Z")), 1);

    let record = read(&root, &opened.id).unwrap();
    assert_eq!(record.ended_by, Some(EndedBy::Unreported));
    assert_eq!(
        Finished::of(&record).map(Finished::word),
        Some("ended without a report")
    );
}

#[test]
fn a_finished_row_is_matched_to_its_asking_chat_by_id_and_never_by_number() {
    // A record that names its asking chat by number alone: after a restart that number is
    // another chat's, so the row is nobody's.
    let (_d, root) = project();
    let numbered = Opening {
        mode: Mode::Task,
        asker: Asker {
            chat: ChatRef {
                id: None,
                ..steward()
            },
            ..a_handoff().asker
        },
        ..a_handoff()
    };
    let opened = open(&root, numbered, at("2026-10-07T12:00:00Z")).unwrap();
    close(
        &root,
        &opened.id,
        Ending {
            report: Some(done("Healthy.")),
            usage: None,
        },
        at("2026-10-07T12:01:00Z"),
    )
    .unwrap();

    // The chat that has that number now, with or without an id of its own.
    assert!(finished_for(&root, &steward(), nobody_open).is_empty());
    let no_id = ChatRef {
        id: None,
        ..steward()
    };
    assert!(finished_for(&root, &no_id, nobody_open).is_empty());
    assert_eq!(clear_for(&root, &no_id), 0);
}

#[test]
fn a_chat_the_person_took_over_is_marked_on_its_record_once() {
    let (_d, root) = project();
    let task = a_finished_task(&root, "check prod", 7, done("Healthy."), 1);
    assert!(!task.kept_open);

    assert!(kept_open(&root, &task.id).unwrap());
    assert!(!kept_open(&root, &task.id).unwrap());

    let kept = read(&root, &task.id).unwrap();
    assert!(kept.kept_open);
    assert!(!serde_json::to_string(&task).unwrap().contains("kept_open"));
    assert!(!serde_json::to_string(&task).unwrap().contains("ended_by"));
}

#[test]
fn a_task_the_person_ended_says_which_way_in_words_of_its_own_and_never_folds() {
    // #1488, V100-5, V100-9. Who ended it and which way are the app's facts, written with the
    // ending: never read from the report's words, so a task cannot make its row read either
    // way by what it says, and cannot make it fold.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let ended_as = |outcome: Outcome, text: &str, by: Option<EndedBy>, way: Option<EndedWay>| {
        let opened = open(
            &root,
            Opening {
                mode: Mode::Task,
                ..a_handoff()
            },
            at("2026-10-08T12:10:00Z"),
        )
        .unwrap();
        close_as(
            &root,
            &opened.id,
            Ending {
                report: Some(ended(outcome, text)),
                usage: None,
            },
            by,
            way,
            at("2026-10-08T12:11:00Z"),
        )
        .unwrap();
        let record = read(&root, &opened.id).unwrap();
        (Finished::of(&record).unwrap(), record)
    };
    let person = Some(EndedBy::Person);

    let (stopped, record) = ended_as(
        Outcome::Done,
        "All done, honest.",
        person,
        Some(EndedWay::Stopped),
    );
    assert_eq!(stopped, Finished::StoppedByThePerson);
    assert_eq!(record.ended_by, person);
    assert_eq!(record.ended_way, Some(EndedWay::Stopped));
    assert_eq!(
        (stopped.word(), stopped.said_to_a_chat(), stopped.key()),
        (
            "stopped by you",
            "stopped by the person",
            "stopped_by_person"
        )
    );
    assert!(!stopped.folds());

    let (closed, record) = ended_as(
        Outcome::Stopped,
        crate::handback::STOPPED,
        person,
        Some(EndedWay::Closed),
    );
    assert_eq!(closed, Finished::ClosedByThePerson);
    assert_eq!(record.ended_way, Some(EndedWay::Closed));
    assert_eq!(
        (closed.word(), closed.said_to_a_chat(), closed.key()),
        ("closed by you", "closed by the person", "closed_by_person")
    );
    assert!(!closed.folds());

    // purlis stopped it at a limit the person set (#1512): its own row, never the person's.
    let (limited, record) = ended_as(
        Outcome::Done,
        "Half of it.",
        Some(EndedBy::Limit),
        Some(EndedWay::Stopped),
    );
    assert_eq!(limited, Finished::StoppedAtItsTimeLimit);
    assert_eq!(record.ended_way, Some(EndedWay::Stopped));
    assert_eq!(
        (limited.word(), limited.said_to_a_chat(), limited.key()),
        (
            "stopped at its time limit",
            "stopped at its time limit",
            "stopped_at_limit"
        )
    );
    assert!(!limited.folds());
    // Which limit is the record's own, kept before its end: a task below it says so.
    let below = Record {
        limit: Some(crate::dispatchlimits::Reached::Above { limit: 30 }),
        ..record
    };
    let how = Finished::of(&below).unwrap();
    assert_eq!(
        (how.word(), how.key()),
        ("stopped with the task above it", "stopped_with_above")
    );

    // A cancel its asking chat asked for stays a cancel, and folds.
    let (cancelled, _) = ended_as(Outcome::Cancelled, "Stopped half way.", None, None);
    assert_eq!((cancelled.word(), cancelled.folds()), ("cancelled", true));

    // A way is the person's alone: beside no other end is one kept, so a record cannot say
    // "stopped by you" of a task nobody stopped.
    let (plain, record) = ended_as(Outcome::Done, "Fine.", None, Some(EndedWay::Stopped));
    assert_eq!(plain, Finished::Done);
    assert_eq!(record.ended_way, None);
    let (died, record) = ended_as(
        Outcome::Failed,
        "x",
        Some(EndedBy::Unreported),
        Some(EndedWay::Stopped),
    );
    assert_eq!(died, Finished::EndedWithoutAReport);
    assert_eq!(record.ended_way, None);

    // On disk: one key, absent where it does not apply.
    assert_eq!(
        serde_json::to_string(&EndedWay::Stopped).unwrap(),
        r#""stopped""#
    );
    assert_eq!(
        serde_json::to_string(&EndedWay::Closed).unwrap(),
        r#""closed""#
    );
    let text = serde_json::to_string(&record).unwrap();
    assert!(!text.contains("ended_way"), "{text}");
}

// ---- the brief a dispatch was sent (#1494) ------------------------------------------------

/// A brief of several lines, with text that looks like markup and characters a careless
/// reader would lose: tabs, a trailing space, an emoji, a combining mark, right-to-left
/// letters, a backslash, a quote, and no line break at its end.
const AN_ODD_BRIEF: &str = "# Check prod\n\n<b>Is</b> the [rollout](https://example.test) \
     healthy?\n\t- `kubectl get pods` \n![img](x.png) <script>alert(1)</script> &amp;\n\
     naïve cafe\u{301} 🚀 שלום \\u202e \"quoted\"  ";

#[test]
fn the_brief_read_back_is_the_brief_as_it_was_sent_byte_for_byte() {
    let (_d, root) = project();
    let opened = open(
        &root,
        Opening {
            mode: Mode::Task,
            brief: AN_ODD_BRIEF.to_owned(),
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();

    let record = read(&root, &opened.id).expect("it reads");
    let sent = brief_sent(&record);
    assert_eq!(sent.kept, BriefKept::Whole);
    assert_eq!(sent.text.as_bytes(), AN_ODD_BRIEF.as_bytes());

    // And the same once the dispatch has ended: a finished task's brief is its record's too.
    close(
        &root,
        &opened.id,
        Ending {
            report: Some(done("Healthy.")),
            usage: None,
        },
        at("2026-10-07T12:04:00Z"),
    )
    .unwrap();
    let ended = read(&root, &opened.id).expect("it reads");
    assert_eq!(brief_sent(&ended).text.as_bytes(), AN_ODD_BRIEF.as_bytes());
}

#[test]
fn a_brief_the_store_cut_is_said_as_cut_and_holds_only_bytes_that_were_sent() {
    let (_d, root) = project();
    // Longer than the store keeps, with the cap inside a two-byte character.
    let long = format!("a{}", "é".repeat(MOST_BRIEF_BYTES));
    let opened = open(
        &root,
        Opening {
            brief: long.clone(),
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();

    let sent = brief_sent(&read(&root, &opened.id).expect("it reads"));
    assert_eq!(sent.kept, BriefKept::Cut);
    assert!(long.starts_with(&sent.text), "only what was sent");
    assert!(
        !sent.text.contains("[cut at"),
        "the store's mark is not the chat's word"
    );
    assert!(sent.text.len() >= MOST_BRIEF_BYTES - 3);

    // The longest brief that can start a chat is whole, and so is one that only quotes the
    // store's mark.
    for whole in [
        "b".repeat(crate::handoff::FIRST_MESSAGE_MAX_BYTES),
        format!("It said: [cut at {MOST_BRIEF_BYTES} bytes]"),
    ] {
        let record = Record {
            brief: whole.clone(),
            ..opened.clone()
        };
        assert_eq!(
            brief_sent(&record),
            SentBrief {
                text: whole,
                kept: BriefKept::Whole
            }
        );
    }
}

#[test]
fn a_record_that_holds_no_brief_says_so() {
    let (_d, root) = project();
    let opened = open(
        &root,
        Opening {
            brief: String::new(),
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();
    assert_eq!(
        brief_sent(&opened),
        SentBrief {
            text: String::new(),
            kept: BriefKept::Missing
        }
    );
}

#[test]
fn a_record_is_read_for_its_brief_whatever_the_brief_holds_and_never_for_a_name_that_misleads() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();

    // A brief that turns its words around is not a record purlis lists, and is one whose
    // brief can still be read: the reader writes that character out.
    let turned = Record {
        brief: "Check prod.\u{202e}dne eht ta eteled dna".to_owned(),
        ..opened.clone()
    };
    assert!(!sound(&turned));
    assert!(sound_but_for_its_brief(&turned));

    // A name that does the same is refused here as everywhere.
    let mut named = opened.clone();
    named.asker.chat.name = "steward\u{202e} 3".to_owned();
    assert!(!sound_but_for_its_brief(&named));
    // And a brief far longer than the store ever writes is not one it wrote.
    let huge = Record {
        brief: "x".repeat(MOST_BRIEF_BYTES * 2),
        ..opened
    };
    assert!(!sound_but_for_its_brief(&huge));
}

// ----- times that do not read as times (#1520) ---------------------------------------------

#[test]
fn a_record_whose_times_do_not_read_as_times_is_counted_and_never_shown() {
    let (_d, root) = project();
    let good = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    let said = |at: &str| Said {
        at: at.to_owned(),
        kind: crate::dispatchtalk::Kind::Note,
        text: "Still up.".to_owned(),
        by: None,
        unread: false,
        left_out: false,
    };
    let bad = [
        planted(&root, |record| record.started = "yesterday".to_owned()),
        planted(&root, |record| record.ended = Some("soon".to_owned())),
        // An end before its start.
        planted(&root, |record| {
            record.ended = Some("2026-10-07T11:00:00+00:00".to_owned());
        }),
        planted(&root, |record| {
            record.messages = 1;
            record.talk = vec![said("2026-13-45T99:00:00+00:00")];
        }),
    ];

    let drawn = drawn(&root);

    assert_eq!(drawn.refused, bad.len());
    let shown: Vec<&str> = drawn.records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(shown, [good.id.as_str()]);
}

#[test]
fn what_a_record_kept_is_taken_out_where_its_end_does_not_read_as_a_time() {
    // When it ended cannot be told, so the 30 days cannot be counted: its words are not kept
    // on without end.
    let (_d, root) = project();
    let opened = open(
        &root,
        Opening {
            mode: Mode::Task,
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();
    said(
        &root,
        &opened.id,
        crate::dispatchtalk::Kind::Note,
        "Still up.",
        at("2026-10-07T12:01:00Z"),
    )
    .unwrap();
    let mut record = read(&root, &opened.id).unwrap();
    record.ended = Some("whenever".to_owned());
    std::fs::write(
        dir(&root).join(format!("{}.json", record.id)),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .unwrap();
    // A running record keeps its words, however its start reads.
    let running = planted(&root, |record| {
        record.mode = Mode::Task;
        record.started = "long ago".to_owned();
        record.messages = 1;
        record.talk = vec![Said {
            at: "2026-10-07T12:01:00+00:00".to_owned(),
            kind: crate::dispatchtalk::Kind::Note,
            text: "Still up.".to_owned(),
            by: None,
            unread: false,
            left_out: false,
        }];
    });

    assert_eq!(expire_talk(&root, at("2026-10-07T12:02:00Z")), 1);

    let kept = read(&root, &opened.id).expect("the record stays");
    assert_eq!(kept.messages, 1);
    assert_eq!(kept.talk[0].text, "");
    assert_eq!(read(&root, &running).unwrap().talk[0].text, "Still up.");
}

/// A task in `beta` that kept one message, with its end planted as `ended`.
fn talked_and_ended_at(root: &Path, ended: &str) -> String {
    let opened = open(
        root,
        Opening {
            mode: Mode::Task,
            ..a_handoff()
        },
        at("2026-10-07T12:00:00Z"),
    )
    .unwrap();
    said(
        root,
        &opened.id,
        crate::dispatchtalk::Kind::Note,
        "Still up.",
        at("2026-10-07T12:00:10Z"),
    )
    .unwrap();
    let mut record = read(root, &opened.id).unwrap();
    record.ended = Some(ended.to_owned());
    std::fs::write(
        dir(root).join(format!("{}.json", opened.id)),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    opened.id
}

#[test]
fn what_a_record_kept_is_taken_out_where_its_end_stands_in_the_future() {
    // A clock stepped forward, or a record written so: its 30 days would never be counted.
    let (_d, root) = project();
    let far = talked_and_ended_at(&root, "9999-12-31T00:00:00+00:00");
    // A minute ahead of this clock is a clock a little ahead, and not yet due.
    let near = talked_and_ended_at(&root, "2026-10-07T12:03:00+00:00");

    assert_eq!(expire_talk(&root, at("2026-10-07T12:02:00Z")), 1);

    assert_eq!(read(&root, &far).unwrap().talk[0].text, "");
    assert_eq!(read(&root, &near).unwrap().talk[0].text, "Still up.");
}

// ----- clearing a row forgets what was said (#1520) ----------------------------------------

/// A finished task of `steward 3`'s that kept one message.
fn a_task_that_talked(root: &Path, name: &str, worker: u32, minute: u32) -> String {
    let opening = Opening {
        mode: Mode::Task,
        task: Some(name.to_owned()),
        worker: Worker {
            chat: ChatRef {
                chat: worker,
                id: Some(mint()),
                name: name.to_owned(),
                persona: Some("devops".to_owned()),
            },
            ..a_handoff().worker
        },
        ..a_handoff()
    };
    let opened = open(root, opening, at(&format!("2026-10-07T12:{minute:02}:00Z"))).unwrap();
    said(
        root,
        &opened.id,
        crate::dispatchtalk::Kind::Note,
        "Still up.",
        at(&format!("2026-10-07T12:{minute:02}:10Z")),
    )
    .unwrap();
    close(
        root,
        &opened.id,
        Ending {
            report: Some(done("Healthy.")),
            usage: None,
        },
        at(&format!("2026-10-07T12:{minute:02}:30Z")),
    )
    .unwrap();
    opened.id
}

#[test]
fn clear_finished_forgets_what_was_said_and_keeps_the_record() {
    let (_d, root) = project();
    let id = a_task_that_talked(&root, "check prod", 7, 1);
    let before = read(&root, &id).unwrap();

    assert!(clear_forgetting(&root, &id).unwrap());
    assert!(!clear_forgetting(&root, &id).unwrap(), "cleared once");

    let after = read(&root, &id).expect("the record stays");
    assert!(after.cleared);
    assert_eq!(after.messages, 1);
    assert_eq!(after.talk[0].text, "", "its words are forgotten");
    assert_eq!(after.talk[0].at, before.talk[0].at, "its line stays");
    assert_eq!(after.brief, before.brief);
    assert_eq!(after.report, before.report);
    assert!(sound(&after));
}

#[test]
fn a_row_taken_for_a_reopen_keeps_what_was_said() {
    // `clear` is the row and nothing else: a Reopen takes the row, and forgets nothing.
    let (_d, root) = project();
    let id = a_task_that_talked(&root, "check prod", 7, 1);

    assert!(clear(&root, &id).unwrap());

    assert_eq!(read(&root, &id).unwrap().talk[0].text, "Still up.");
}

#[test]
fn a_chat_that_closes_forgets_what_its_finished_tasks_said() {
    let (_d, root) = project();
    let one = a_task_that_talked(&root, "check prod", 7, 1);
    let two = a_task_that_talked(&root, "check staging", 8, 2);

    assert_eq!(clear_for(&root, &steward()), 2);

    for id in [&one, &two] {
        let kept = read(&root, id).expect("the record stays");
        assert!(kept.cleared);
        assert_eq!(kept.talk[0].text, "");
        assert!(kept.report.is_some());
    }
}

#[test]
fn a_removed_workspace_s_tasks_forget_what_they_said_and_their_records_are_counted() {
    let (_d, root) = project();
    // `a_handoff` works in `beta`; one task there talked and ended, one runs on.
    let ended = a_task_that_talked(&root, "check prod", 7, 1);
    let running = open(
        &root,
        Opening {
            mode: Mode::Task,
            ..a_handoff()
        },
        at("2026-10-07T12:05:00Z"),
    )
    .unwrap();
    said(
        &root,
        &running.id,
        crate::dispatchtalk::Kind::Note,
        "Still up.",
        at("2026-10-07T12:05:10Z"),
    )
    .unwrap();
    // Another workspace's is not touched.
    let elsewhere = planted(&root, |record| {
        record.place.workspace = Some("alpha".to_owned());
        record.messages = 1;
        record.talk = vec![Said {
            at: "2026-10-07T12:01:00+00:00".to_owned(),
            kind: crate::dispatchtalk::Kind::Note,
            text: "Elsewhere.".to_owned(),
            by: None,
            unread: false,
            left_out: false,
        }];
        record.ended = Some("2026-10-07T12:02:00+00:00".to_owned());
    });

    let left = workspace_removed(&root, "beta").unwrap();

    assert_eq!(
        left,
        LeftBehind {
            records: 2,
            forgot: 1,
            running: 1,
        }
    );
    assert_eq!(read(&root, &ended).unwrap().talk[0].text, "");
    assert_eq!(read(&root, &running.id).unwrap().talk[0].text, "Still up.");
    assert_eq!(read(&root, &elsewhere).unwrap().talk[0].text, "Elsewhere.");
    // No store at all leaves nothing.
    let (_e, empty) = project();
    assert_eq!(
        workspace_removed(&empty, "beta").unwrap(),
        LeftBehind::default()
    );
}

/// #1519: a handoff an older build opened, which asked for a report and has not sent it, is a
/// task from the next launch on: its report ends it as a task's does, and it is a finished row
/// under the chat that asked. A handoff that asked for nothing, or has ended, is as it was.
#[test]
fn a_running_handoff_an_older_build_opened_owing_a_report_is_a_task_from_the_next_launch() {
    let (_d, root) = project();
    // What the older build wrote: a handoff that asked for a report, still running.
    let owed = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    let asked_nothing = open(
        &root,
        Opening {
            report_owed: false,
            ..a_handoff()
        },
        at("2026-10-07T12:00:01Z"),
    )
    .unwrap();
    let ended = open(&root, a_handoff(), at("2026-10-07T12:00:02Z")).unwrap();
    close(
        &root,
        &ended.id,
        Ending {
            report: Some(done("Sent before the update.")),
            usage: None,
        },
        at("2026-10-07T12:30:00Z"),
    )
    .unwrap();
    // Its persona chat comes back at this launch, still owing its report.
    older_chat_back(&root, crate::reopen::Owed::Due);

    settle_on_open(&root, at("2026-10-08T09:00:00Z"));

    let mode = |id: &str| read(&root, id).map(|record| record.mode);
    assert_eq!(mode(&owed.id), Some(Mode::Task));
    assert_eq!(mode(&asked_nothing.id), Some(Mode::Handoff));
    assert_eq!(mode(&ended.id), Some(Mode::Handoff));
    assert!(
        read(&root, &owed.id).unwrap().running(),
        "its chat came back"
    );

    close(
        &root,
        &owed.id,
        Ending {
            report: Some(done("Healthy: 3 of 3 pods ready.")),
            usage: None,
        },
        at("2026-10-08T09:10:00Z"),
    )
    .unwrap();
    let record = read(&root, &owed.id).unwrap();
    assert_eq!(Finished::of(&record), Some(Finished::Done));
    assert_eq!(Finished::of(&read(&root, &ended.id).unwrap()), None);
}

#[test]
fn a_handoff_an_older_build_opened_whose_chat_is_gone_ends_as_a_task_that_did_not_report() {
    let (_d, root) = project();
    let owed = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();

    assert_eq!(settle_on_open(&root, at("2026-10-08T09:00:00Z")), 1);

    let record = read(&root, &owed.id).unwrap();
    assert_eq!(
        Finished::of(&record),
        Some(Finished::EndedWithoutAReport),
        "{record:?}"
    );
}

/// Review F6: the older build delivered the report and did not get to end the dispatch record.
/// The chat's own record says sent, so the dispatch stays the handoff it was, and is never
/// shown as a task that did not report.
#[test]
fn an_older_handoff_whose_chat_says_it_reported_is_not_made_a_task() {
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    older_chat_back(&root, crate::reopen::Owed::Sent);

    assert_eq!(settle_on_open(&root, at("2026-10-08T09:00:00Z")), 0);

    let record = read(&root, &opened.id).unwrap();
    assert_eq!(record.mode, Mode::Handoff);
    assert!(record.running(), "its chat came back");
    // The same record, its chat still owing: a task.
    older_chat_back(&root, crate::reopen::Owed::Due);
    settle_on_open(&root, at("2026-10-08T09:00:00Z"));
    assert_eq!(read(&root, &opened.id).unwrap().mode, Mode::Task);
}

/// A reopen record bringing back the `devops` chat an older build handed off from `steward 3`,
/// whose record says its report is `owed`.
fn older_chat_back(root: &Path, owed: crate::reopen::Owed) {
    let record = crate::reopen::Record {
        chats: vec![crate::reopen::Chat {
            program: "claude".into(),
            number: Some(devops().chat),
            identity: crate::reopen::Identity {
                id: devops().id,
                ..Default::default()
            },
            from: Some(crate::reopen::HandedFrom {
                chat: steward().chat,
                name: steward().name,
                workspace: crate::active::Place::Workspace("alpha".to_owned()),
                report: owed,
                mode: crate::reopen::Mode::Handoff,
                depth: 1,
                root: None,
                above: None,
                by_person: false,
            }),
            ..Default::default()
        }],
        dealt: devops().chat,
        ..Default::default()
    };
    crate::reopen::write(root, &record).unwrap();
}

#[test]
fn the_persona_chat_s_newest_session_record_is_the_one_its_dispatch_names() {
    // #1456, #1513: a report after a relaunch names the record the task wrote last.
    let (_d, root) = project();
    let opened = open(&root, a_handoff(), at("2026-10-07T12:00:00Z")).unwrap();
    session_recorded(&root, &devops(), "workspaces/beta/sessions/first.md");
    session_recorded(&root, &devops(), "workspaces/beta/sessions/second.md");

    let record = read(&root, &opened.id).unwrap();
    assert_eq!(
        record.worker.session_record.as_deref(),
        Some("workspaces/beta/sessions/second.md")
    );
    assert!(sound(&record));
}
