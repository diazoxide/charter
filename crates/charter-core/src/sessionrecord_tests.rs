use std::path::Path;

use chrono::NaiveDate;

use super::*;
use crate::active::Place;

/// A plane with `charter.toml` and the named workspaces, each with charter's own template.
fn plane(workspaces: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "").unwrap();
    for ws in workspaces {
        let found = crate::workspaces::Plane::open(dir.path())
            .workspace(ws)
            .unwrap();
        std::fs::create_dir_all(found.dir()).unwrap();
        found.scaffold_charter().unwrap();
    }
    dir
}

fn at(h: u32, m: u32, s: u32) -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 9, 28)
        .unwrap()
        .and_hms_opt(h, m, s)
        .unwrap()
}

const BODY: &str = "## Goal\n\nShip the record.\n\n## Done\n\n- wrote it\n\n## Decisions\n\n- \
one file per record\n\n## Open\n\nNothing.\n\n## How to resume\n\nRead this, then run the tests.\n";

fn facts(place: Place, when: chrono::NaiveDateTime) -> Facts {
    Facts {
        place,
        at: when,
        chat: Some(ChatFacts {
            number: 3,
            name: Some("steward 3".to_owned()),
            harness: Some("claude".to_owned()),
            profile: Some("work".to_owned()),
            conversation: Some("0f6c2a1e-aaaa-bbbb-cccc-123456789abc".to_owned()),
            cwd: Some("workspaces/alpha/charter/si-8b".to_owned()),
        }),
        persona: Some("steward".to_owned()),
        pieces: vec![Touched {
            repo: "charter".to_owned(),
            piece: "si-8b".to_owned(),
            branch: Some("si-8b-session-record-core".to_owned()),
        }],
    }
}

fn alpha() -> Place {
    Place::Workspace("alpha".to_owned())
}

fn write(root: &Path, title: &str, body: &str, facts: &Facts) -> Result<Recorded, Refused> {
    record(root, &New { title, body, facts })
}

// ---- the shape a record must have ---------------------------------------------------------

#[test]
fn a_body_with_the_five_sections_in_order_is_a_record() {
    assert_eq!(check("Ship it", BODY), Ok(()));
}

#[test]
fn a_body_missing_a_section_is_refused_naming_it() {
    let body = BODY.replace("## Open\n\nNothing.\n\n", "");
    let refused = check("Ship it", &body).unwrap_err();
    assert!(refused.contains("Open"));
}

#[test]
fn a_body_with_its_sections_out_of_order_is_refused() {
    let body = "## Done\n\nx\n\n## Goal\n\nx\n\n## Decisions\n\nx\n\n## Open\n\nx\n\n## How to \
                resume\n\nx\n";
    assert!(check("Ship it", body).is_err());
}

#[test]
fn a_body_with_a_section_of_its_own_is_refused() {
    let body = format!("{BODY}\n## Notes\n\nx\n");
    let refused = check("Ship it", &body).unwrap_err();
    assert!(refused.contains("Notes"));
}

#[test]
fn an_empty_section_is_refused() {
    let body = BODY.replace("Nothing.", "");
    let refused = check("Ship it", &body).unwrap_err();
    assert!(refused.contains("Open"));
}

#[test]
fn text_before_the_first_section_is_refused() {
    let body = format!("# Ship it\n\n{BODY}");
    assert!(check("Ship it", &body).is_err());
}

#[test]
fn a_heading_inside_a_code_fence_is_text_and_not_a_section() {
    let body = BODY.replace("Nothing.", "Nothing.\n\n```md\n## Not a section\n```");
    assert_eq!(check("Ship it", &body), Ok(()));
}

#[test]
fn a_title_that_is_empty_multi_line_or_too_long_is_refused() {
    assert!(check("   ", BODY).is_err());
    assert!(check("one\ntwo", BODY).is_err());
    assert!(check(&"x".repeat(MOST_TITLE_CHARS + 1), BODY).is_err());
    assert_eq!(check(&"x".repeat(MOST_TITLE_CHARS), BODY), Ok(()));
}

#[test]
fn a_control_character_in_the_body_is_refused() {
    let body = BODY.replace("Nothing.", "Nothing\u{1b}[2J.");
    assert!(check("Ship it", &body).is_err());
    let body = BODY.replace("Nothing.", "Nothing\u{202e}.");
    assert!(check("Ship it", &body).is_err());
}

#[test]
fn a_body_over_the_size_cap_is_refused() {
    let body = BODY.replace("Nothing.", &"x".repeat(MOST_BODY_BYTES));
    let refused = check("Ship it", &body).unwrap_err();
    assert!(refused.contains("bytes"));
}

#[test]
fn a_record_that_looks_like_it_holds_a_credential_is_refused_by_kind_never_by_value() {
    let body = BODY.replace("Nothing.", "password: hunter2hunter2");
    let refused = check("Ship it", &body).unwrap_err();
    assert!(!refused.contains("hunter2"));
}

// ---- where a record goes, and what it says --------------------------------------------------

#[test]
fn a_workspace_record_is_one_file_under_its_sessions_directory_named_by_time_and_title() {
    let dir = plane(&["alpha"]);
    let done = write(
        dir.path(),
        "Ship the record",
        BODY,
        &facts(alpha(), at(14, 3, 12)),
    )
    .unwrap();
    assert_eq!(
        done.path,
        dir.path()
            .join("workspaces/alpha/sessions/20260928-140312-ship-the-record.md")
    );
    assert_eq!(
        done.shown,
        "workspaces/alpha/sessions/20260928-140312-ship-the-record.md"
    );
    let text = std::fs::read_to_string(&done.path).unwrap();
    assert!(
        text.starts_with(
            "---\ntitle: Ship the record\ndate: 2026-09-28 14:03:12\nchat: 3\nchat-name: \
             steward 3\npersona: steward\nharness: claude\nprofile: work\nconversation: \
             0f6c2a1e-aaaa-bbbb-cccc-123456789abc\nworkspace: alpha\ncwd: \
             workspaces/alpha/charter/si-8b\npiece: charter/si-8b @ \
             si-8b-session-record-core\n---\n\n# Ship the record\n\n## Goal\n"
        ),
        "{text}"
    );
    assert!(text.ends_with("run the tests.\n"), "{text}");
}

#[test]
fn a_fact_charter_does_not_have_is_written_as_unknown_and_never_left_out() {
    let dir = plane(&["alpha"]);
    let mut bare = facts(alpha(), at(9, 0, 0));
    bare.chat = None;
    bare.persona = None;
    bare.pieces.clear();
    let done = write(dir.path(), "Bare", BODY, &bare).unwrap();
    let text = std::fs::read_to_string(&done.path).unwrap();
    for line in [
        "chat: unknown\n",
        "chat-name: unknown\n",
        "persona: none\n",
        "harness: unknown\n",
        "profile: unknown\n",
        "conversation: unknown\n",
        "workspace: alpha\n",
        "cwd: unknown\n",
    ] {
        assert!(text.contains(line), "{line:?} in {text}");
    }
    assert!(!text.contains("piece:"), "{text}");
}

#[test]
fn a_plane_root_record_goes_in_the_planes_own_sessions_directory() {
    let dir = plane(&[]);
    let done = write(
        dir.path(),
        "Tidy personas",
        BODY,
        &facts(Place::PlaneRoot, at(8, 1, 2)),
    )
    .unwrap();
    assert_eq!(done.shown, "sessions/20260928-080102-tidy-personas.md");
    let text = std::fs::read_to_string(&done.path).unwrap();
    assert!(text.contains("\nworkspace: plane root\n"), "{text}");
    assert!(dir.path().join("sessions/index.md").is_file());
}

#[test]
fn a_workspace_this_plane_does_not_have_is_refused_and_nothing_is_written() {
    let dir = plane(&["alpha"]);
    let refused = write(
        dir.path(),
        "Ship",
        BODY,
        &facts(Place::Workspace("ghost".to_owned()), at(1, 1, 1)),
    );
    assert!(
        matches!(refused, Err(Refused::NoWorkspace(_))),
        "{refused:?}"
    );
    assert!(!dir.path().join("workspaces/ghost").exists());
}

#[test]
fn two_records_in_the_same_second_with_the_same_title_are_two_files() {
    let dir = plane(&["alpha"]);
    let one = write(dir.path(), "Same", BODY, &facts(alpha(), at(1, 2, 3))).unwrap();
    let two = write(dir.path(), "Same", BODY, &facts(alpha(), at(1, 2, 3))).unwrap();
    assert_ne!(one.path, two.path);
    assert_eq!(list(dir.path(), &alpha()).len(), 2);
}

#[test]
fn a_shape_refusal_writes_nothing() {
    let dir = plane(&["alpha"]);
    let refused = write(
        dir.path(),
        "Ship",
        "## Goal\n\nx\n",
        &facts(alpha(), at(1, 1, 1)),
    );
    assert!(matches!(refused, Err(Refused::Shape(_))), "{refused:?}");
    assert!(!dir.path().join("workspaces/alpha/sessions").exists());
}

// ---- the index charter keeps -----------------------------------------------------------------

#[test]
fn the_index_lists_every_record_newest_first_one_line_each() {
    let dir = plane(&["alpha"]);
    write(dir.path(), "First", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    write(dir.path(), "Third", BODY, &facts(alpha(), at(11, 0, 0))).unwrap();
    write(dir.path(), "Second", BODY, &facts(alpha(), at(10, 0, 0))).unwrap();

    let index =
        std::fs::read_to_string(dir.path().join("workspaces/alpha/sessions/index.md")).unwrap();
    let lines: Vec<&str> = index.lines().filter(|l| l.starts_with("- ")).collect();
    assert_eq!(
        lines,
        [
            "- 2026-09-28 11:00 · [Third](20260928-110000-third.md)",
            "- 2026-09-28 10:00 · [Second](20260928-100000-second.md)",
            "- 2026-09-28 09:00 · [First](20260928-090000-first.md)",
        ]
    );
    let titles: Vec<String> = list(dir.path(), &alpha())
        .into_iter()
        .map(|l| l.title)
        .collect();
    assert_eq!(titles, ["Third", "Second", "First"]);
}

#[test]
fn the_index_is_rebuilt_from_the_records_so_a_hand_edit_to_it_does_not_survive() {
    let dir = plane(&["alpha"]);
    write(dir.path(), "First", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    let index = dir.path().join("workspaces/alpha/sessions/index.md");
    std::fs::write(&index, "- a line somebody wrote\n").unwrap();
    write(dir.path(), "Second", BODY, &facts(alpha(), at(10, 0, 0))).unwrap();
    let text = std::fs::read_to_string(&index).unwrap();
    assert!(!text.contains("somebody"), "{text}");
    assert_eq!(text.lines().filter(|l| l.starts_with("- ")).count(), 2);
}

#[test]
fn a_title_with_brackets_does_not_break_its_index_link() {
    let dir = plane(&["alpha"]);
    write(
        dir.path(),
        "Fix [the] bug",
        BODY,
        &facts(alpha(), at(9, 0, 0)),
    )
    .unwrap();
    let index =
        std::fs::read_to_string(dir.path().join("workspaces/alpha/sessions/index.md")).unwrap();
    assert!(
        index.contains("[Fix \\[the\\] bug](20260928-090000-fix-the-bug.md)"),
        "{index}"
    );
}

// ---- the pointer in workspace.md -------------------------------------------------------------

#[test]
fn workspace_md_points_at_the_index_with_the_count_and_the_latest_record() {
    let dir = plane(&["alpha"]);
    write(dir.path(), "First", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    write(dir.path(), "Second", BODY, &facts(alpha(), at(10, 0, 0))).unwrap();
    let text = std::fs::read_to_string(dir.path().join("workspaces/alpha/workspace.md")).unwrap();
    let body = crate::mdsection::section_body(&text, "Sessions");
    assert_eq!(
        body,
        "2 session records — the latest is [Second](sessions/20260928-100000-second.md) \
         (2026-09-28 10:00); all of them, newest first, in \
         [sessions/index.md](sessions/index.md)."
    );
}

#[test]
fn the_pointer_is_rewritten_in_place_and_every_other_section_is_left_as_it_was() {
    let dir = plane(&["alpha"]);
    let path = dir.path().join("workspaces/alpha/workspace.md");
    let before = std::fs::read_to_string(&path).unwrap();
    write(dir.path(), "First", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    let once = std::fs::read_to_string(&path).unwrap();
    assert_eq!(once.matches("## Sessions").count(), 1, "{once}");
    assert_eq!(
        crate::mdsection::section_body(&once, "Glossary"),
        crate::mdsection::section_body(&before, "Glossary")
    );
    assert_eq!(
        crate::mdsection::section_body(&once, "Log"),
        crate::mdsection::section_body(&before, "Log")
    );
    // Asked again with nothing new, it writes the same bytes.
    point(dir.path(), "alpha").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), once);
}

#[test]
fn a_workspace_md_written_before_sessions_existed_gains_the_section() {
    let dir = plane(&["alpha"]);
    let path = dir.path().join("workspaces/alpha/workspace.md");
    std::fs::write(&path, "# alpha\n\n## Vision\n\nShip.\n").unwrap();
    write(dir.path(), "First", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with("# alpha\n\n## Vision\n\nShip.\n"),
        "{text}"
    );
    assert!(
        crate::mdsection::section_body(&text, "Sessions").starts_with("1 session record —"),
        "{text}"
    );
}

#[test]
fn a_fresh_workspace_md_has_a_sessions_section_saying_there_are_none_yet() {
    let dir = plane(&["alpha"]);
    let text = std::fs::read_to_string(dir.path().join("workspaces/alpha/workspace.md")).unwrap();
    assert!(
        crate::mdsection::section_body(&text, "Sessions").starts_with("_No session records yet"),
        "{text}"
    );
}

// ---- reading them back -----------------------------------------------------------------------

#[test]
fn the_latest_is_the_newest_record_of_that_place_and_none_where_there_are_none() {
    let dir = plane(&["alpha", "beta"]);
    assert_eq!(latest(dir.path(), &alpha()), None);
    write(dir.path(), "Old", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    write(dir.path(), "New", BODY, &facts(alpha(), at(10, 0, 0))).unwrap();
    let newest = latest(dir.path(), &alpha()).unwrap();
    assert_eq!(newest.title, "New");
    assert_eq!(
        newest.shown,
        "workspaces/alpha/sessions/20260928-100000-new.md"
    );
    assert_eq!(
        latest(dir.path(), &Place::Workspace("beta".to_owned())),
        None
    );
    assert_eq!(latest(dir.path(), &Place::PlaneRoot), None);
}

#[test]
fn a_file_that_is_not_a_record_is_not_listed() {
    let dir = plane(&["alpha"]);
    write(dir.path(), "Real", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    let sessions = dir.path().join("workspaces/alpha/sessions");
    std::fs::write(sessions.join("notes.md"), "x").unwrap();
    std::fs::write(sessions.join("20260928-1-short.md"), "x").unwrap();
    assert_eq!(list(dir.path(), &alpha()).len(), 1);
}

#[test]
fn show_reads_a_record_by_its_file_name_and_refuses_a_path_that_leaves_the_directory() {
    let dir = plane(&["alpha"]);
    let done = write(dir.path(), "Real", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    let name = done.path.file_name().unwrap().to_str().unwrap().to_owned();
    let text = show(dir.path(), &alpha(), &name).unwrap();
    assert!(text.contains("# Real"), "{text}");
    assert!(show(dir.path(), &alpha(), "../workspace.md").is_err());
    assert!(show(dir.path(), &alpha(), "index.md").is_err());
    assert!(show(dir.path(), &alpha(), "20260928-090000-missing.md").is_err());
}

// ---- the chat's own facts, from the app's record ---------------------------------------------

#[test]
fn a_chats_facts_come_from_the_apps_record_of_it_by_number() {
    let dir = plane(&[]);
    std::fs::create_dir_all(dir.path().join(".charter/app")).unwrap();
    std::fs::write(
        dir.path().join(crate::reopen::IN_PLANE),
        r#"{"version": 1, "at": 1, "dealt": 7, "chats": [
            {"program": "claude", "name": "3", "number": 3, "resume": "old-one"},
            {"program": "/usr/local/bin/claude", "name": "7", "number": 7,
             "resume": "0f6c2a1e-aaaa-bbbb-cccc-123456789abc", "persona": "steward",
             "profile": "work", "cwd": "PLANE/workspaces/alpha/charter/si-8b"}
        ]}"#
        .replace("PLANE", &dir.path().display().to_string()),
    )
    .unwrap();
    let seen = chat_facts(dir.path(), 7);
    assert_eq!(seen.number, 7);
    assert_eq!(seen.name.as_deref(), Some("steward 7"));
    assert_eq!(seen.harness.as_deref(), Some("claude"));
    assert_eq!(
        seen.conversation.as_deref(),
        Some("0f6c2a1e-aaaa-bbbb-cccc-123456789abc")
    );
    assert_eq!(
        seen.profile.as_deref(),
        Some("work"),
        "the profile, by name"
    );
    assert_eq!(
        seen.cwd.as_deref(),
        Some("workspaces/alpha/charter/si-8b"),
        "the directory, plane-relative"
    );
}

#[test]
fn a_chat_at_the_plane_root_ran_in_dot_and_one_outside_the_plane_in_no_directory_charter_says() {
    let dir = plane(&[]);
    std::fs::create_dir_all(dir.path().join(".charter/app")).unwrap();
    std::fs::write(
        dir.path().join(crate::reopen::IN_PLANE),
        r#"{"version": 1, "at": 1, "dealt": 7, "chats": [
            {"program": "claude", "name": "3", "number": 3, "cwd": "PLANE"},
            {"program": "claude", "name": "4", "number": 4, "cwd": "/somewhere/else"},
            {"program": "claude", "name": "5", "number": 5, "cwd": "PLANE/workspaces/../.."}
        ]}"#
        .replace("PLANE", &dir.path().display().to_string()),
    )
    .unwrap();
    assert_eq!(chat_facts(dir.path(), 3).cwd.as_deref(), Some("."));
    assert_eq!(chat_facts(dir.path(), 4).cwd, None, "outside the plane");
    assert_eq!(chat_facts(dir.path(), 5).cwd, None, "a path that walks up");
}

#[test]
fn a_chat_the_app_has_no_record_of_is_known_only_by_its_number() {
    let dir = plane(&[]);
    let seen = chat_facts(dir.path(), 5);
    assert_eq!(
        seen,
        ChatFacts {
            number: 5,
            name: None,
            harness: None,
            profile: None,
            conversation: None,
            cwd: None,
        }
    );
}

// ---- reading one back by its plane-relative path (SI-8d) ------------------------------------

#[test]
fn a_listing_carries_each_records_persona_harness_and_conversation() {
    let dir = plane(&["alpha"]);
    write(dir.path(), "Known", BODY, &facts(alpha(), at(10, 0, 0))).unwrap();
    let mut bare = facts(alpha(), at(9, 0, 0));
    bare.chat = None;
    bare.persona = None;
    write(dir.path(), "Bare", BODY, &bare).unwrap();
    let listed = list(dir.path(), &alpha());
    assert_eq!(listed[0].title, "Known");
    assert_eq!(listed[0].persona.as_deref(), Some("steward"));
    assert_eq!(listed[0].harness.as_deref(), Some("claude"));
    assert_eq!(
        listed[0].conversation.as_deref(),
        Some("0f6c2a1e-aaaa-bbbb-cccc-123456789abc")
    );
    // `unknown` and `none` are charter's words for a fact it did not have, never a value.
    assert_eq!(listed[1].title, "Bare");
    assert_eq!(listed[1].persona, None);
    assert_eq!(listed[1].harness, None);
    assert_eq!(listed[1].conversation, None);
    assert_eq!(listed[0].profile.as_deref(), Some("work"));
    assert_eq!(
        listed[0].cwd.as_deref(),
        Some("workspaces/alpha/charter/si-8b")
    );
    assert_eq!(listed[1].profile, None);
    assert_eq!(listed[1].cwd, None);
}

#[test]
fn a_profile_or_directory_a_hand_edit_made_unsafe_is_not_carried() {
    let dir = plane(&["alpha"]);
    let done = write(dir.path(), "Edited", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    let original = std::fs::read_to_string(&done.path).unwrap();
    for (profile, cwd) in [
        ("--dangerously-skip-permissions", "../../etc"),
        ("work;rm", "/etc"),
        ("wo.rk", "workspaces/alpha/../../.."),
        ("work", "workspaces/alpha/./x"),
        ("work", "workspaces\\alpha"),
        ("work", "workspaces//alpha"),
        ("work", "workspaces/alpha/"),
    ] {
        let text = original
            .replace("profile: work", &format!("profile: {profile}"))
            .replace(
                "cwd: workspaces/alpha/charter/si-8b",
                &format!("cwd: {cwd}"),
            );
        std::fs::write(&done.path, text).unwrap();
        let listed = list(dir.path(), &alpha());
        if profile != "work" {
            assert_eq!(listed[0].profile, None, "not a profile name");
        }
        assert_eq!(listed[0].cwd, None, "not a plane-relative directory");
    }
}

#[test]
fn a_conversation_id_a_hand_edit_made_unsafe_is_not_carried() {
    let dir = plane(&["alpha"]);
    let done = write(dir.path(), "Edited", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    let text = std::fs::read_to_string(&done.path).unwrap().replace(
        "conversation: 0f6c2a1e-aaaa-bbbb-cccc-123456789abc",
        "conversation: --dangerously-skip-permissions",
    );
    std::fs::write(&done.path, text).unwrap();
    let listed = list(dir.path(), &alpha());
    assert_eq!(listed[0].conversation, None, "a flag is never an id");
}

#[test]
fn locate_reads_a_plane_relative_record_path() {
    assert_eq!(
        locate("sessions/20260928-140312-ship-it.md"),
        Ok((Place::PlaneRoot, "20260928-140312-ship-it.md".to_owned()))
    );
    assert_eq!(
        locate("workspaces/alpha/sessions/20260928-140312-ship-it.md"),
        Ok((alpha(), "20260928-140312-ship-it.md".to_owned()))
    );
}

#[test]
fn locate_refuses_every_path_that_is_not_one_records_own() {
    for path in [
        "",
        "20260928-140312-ship-it.md",
        "/etc/passwd",
        "sessions/index.md",
        "sessions/../charter.toml",
        "sessions/../../20260928-140312-ship-it.md",
        "../sessions/20260928-140312-ship-it.md",
        "/sessions/20260928-140312-ship-it.md",
        "workspaces/../sessions/20260928-140312-ship-it.md",
        "workspaces/alpha/../beta/sessions/20260928-140312-ship-it.md",
        "workspaces/alpha/sessions/../../../20260928-140312-ship-it.md",
        "workspaces/alpha/sessions/sub/20260928-140312-ship-it.md",
        "workspaces/alpha/memory/20260928-140312-ship-it.md",
        "workspaces/.hidden/sessions/20260928-140312-ship-it.md",
        "sessions\\..\\20260928-140312-ship-it.md",
        "sessions/20260928-140312-ship-it.md/",
        "./sessions/20260928-140312-ship-it.md",
    ] {
        assert!(
            locate(path).is_err(),
            "a path that is not a record's was taken"
        );
    }
}

#[test]
fn open_reads_a_record_by_its_path_with_its_facts_and_its_text() {
    let dir = plane(&["alpha"]);
    let done = write(dir.path(), "Real", BODY, &facts(alpha(), at(9, 0, 0))).unwrap();
    let opened = open(dir.path(), &done.shown).unwrap();
    assert_eq!(opened.place, alpha());
    assert_eq!(opened.listed.title, "Real");
    assert_eq!(opened.listed.shown, done.shown);
    assert_eq!(opened.listed.harness.as_deref(), Some("claude"));
    assert!(opened.text.contains("## How to resume"), "the whole text");
    assert!(
        opened.body().starts_with("# Real\n"),
        "the body is what follows charter's frontmatter"
    );
    assert!(!opened.body().contains("conversation:"));
}

#[test]
fn open_refuses_a_path_that_leaves_the_sessions_directory_and_a_record_that_is_not_there() {
    let dir = plane(&["alpha"]);
    std::fs::write(dir.path().join("secret.md"), "x").unwrap();
    assert!(open(dir.path(), "sessions/../secret.md").is_err());
    assert!(open(dir.path(), "workspaces/alpha/workspace.md").is_err());
    assert!(
        open(
            dir.path(),
            "workspaces/alpha/sessions/20260928-090000-missing.md"
        )
        .is_err()
    );
    assert!(
        open(
            dir.path(),
            "workspaces/nope/sessions/20260928-090000-missing.md"
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn open_refuses_a_plane_root_record_that_is_a_link() {
    let dir = plane(&[]);
    std::fs::create_dir_all(dir.path().join(DIR)).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("x.md");
    std::fs::write(&target, "---\ntitle: x\n---\n").unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("sessions/20260928-090000-x.md")).unwrap();
    assert!(open(dir.path(), "sessions/20260928-090000-x.md").is_err());
}

// ---- the record a saved line names ----------------------------------------------------------

#[test]
fn a_saved_line_names_its_record_by_path_and_the_window_is_told_its_title() {
    let tmp = plane(&["alpha"]);
    let recorded = write(tmp.path(), "Ship it", BODY, &facts(alpha(), at(14, 3, 12))).unwrap();

    let named = saved(tmp.path(), &recorded.path).expect("the record");

    assert_eq!(
        named.shown,
        "workspaces/alpha/sessions/20260928-140312-ship-it.md"
    );
    assert_eq!(named.title, "Ship it");
}

#[test]
fn a_saved_line_naming_anything_but_one_of_this_planes_records_names_nothing() {
    let tmp = plane(&["alpha"]);
    let recorded = write(tmp.path(), "Ship it", BODY, &facts(alpha(), at(14, 3, 12))).unwrap();
    let other = tempfile::tempdir().unwrap();
    for path in [
        other.path().join("sessions/20260928-140312-ship-it.md"),
        tmp.path().join("workspaces/alpha/workspace.md"),
        tmp.path()
            .join("workspaces/alpha/sessions/../sessions/20260928-140312-ship-it.md"),
        tmp.path()
            .join("workspaces/alpha/sessions/20260928-150000-gone.md"),
    ] {
        assert_eq!(
            saved(tmp.path(), &path),
            None,
            "a path that is no record named one"
        );
    }
    assert!(saved(tmp.path(), &recorded.path).is_some());
}

#[cfg(unix)]
#[test]
fn a_saved_line_spelling_the_plane_another_way_still_names_its_record() {
    let tmp = plane(&["alpha"]);
    let recorded = write(tmp.path(), "Ship it", BODY, &facts(alpha(), at(14, 3, 12))).unwrap();
    let other = tempfile::tempdir().unwrap();
    let link = other.path().join("plane");
    std::os::unix::fs::symlink(tmp.path(), &link).unwrap();

    let named = saved(&link, &recorded.path).expect("the record, through the other spelling");

    assert_eq!(named.title, "Ship it");
}
