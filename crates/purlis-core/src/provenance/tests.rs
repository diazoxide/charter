use super::*;

const CHAT: &str = "01J9ZQ3W5Y7X8V6T4R2P0N1M3K";

fn everything() -> Provenance {
    Provenance {
        harness: Some("claude-code".into()),
        model: Some("claude-opus-5-5".into()),
        chat: Some(CHAT.into()),
        persona: Some("steward".into()),
        change: Some("billing-v2".into()),
    }
}

#[test]
fn an_agent_commit_carries_all_four_trailers_in_v67s_order_and_spelling() {
    assert_eq!(
        everything().trailers(Form::Full),
        [
            "Assisted-by: claude-code:claude-opus-5-5",
            "Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K",
            "Purlis-Persona: steward",
            "Purlis-Change: billing-v2",
        ]
    );
}

#[test]
fn the_kernel_form_names_no_harness_and_no_model() {
    assert_eq!(everything().trailers(Form::Llm)[0], "Assisted-by: LLM");
}

#[test]
fn a_model_charter_does_not_know_leaves_the_harness_alone_with_no_colon() {
    let p = Provenance {
        model: None,
        ..everything()
    };
    assert_eq!(p.trailers(Form::Full)[0], "Assisted-by: claude-code");
}

#[test]
fn a_trailer_whose_value_is_unknown_is_left_out() {
    let p = Provenance {
        persona: None,
        change: None,
        ..everything()
    };
    assert_eq!(
        p.trailers(Form::Full),
        [
            "Assisted-by: claude-code:claude-opus-5-5",
            "Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K",
        ]
    );
}

#[test]
fn no_harness_is_not_an_agent_run_and_gets_no_trailer_at_all() {
    let p = Provenance {
        harness: None,
        ..everything()
    };
    assert!(p.trailers(Form::Full).is_empty());
    assert!(p.trailers(Form::Llm).is_empty());
}

#[test]
fn a_value_that_would_break_the_trailer_line_is_unknown_and_left_out() {
    for bad in [
        "",
        "two words",
        "new\nline",
        "Charter-Chat: x",
        "tab\there",
        "é",
    ] {
        let p = Provenance {
            model: Some(bad.into()),
            persona: Some(bad.into()),
            change: Some(bad.into()),
            chat: Some(bad.into()),
            ..everything()
        };
        assert_eq!(
            p.trailers(Form::Full),
            ["Assisted-by: claude-code"],
            "{bad:?}"
        );
    }
}

fn plane_with(chats: Vec<crate::reopen::Chat>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    crate::reopen::write(
        dir.path(),
        &crate::reopen::Record {
            chats,
            dealt: 9,
            ..Default::default()
        },
    )
    .unwrap();
    dir
}

/// Chat `number`, started on harness profile `profile` (`None`: a shell or a program started
/// on no profile).
fn chat(
    number: u32,
    profile: Option<&str>,
    persona: Option<&str>,
    id: Option<&str>,
) -> crate::reopen::Chat {
    crate::reopen::Chat {
        program: "/bin/zsh".into(),
        profile: profile.map(str::to_owned),
        name: format!("chat {number}"),
        number: Some(number),
        persona: persona.map(str::to_owned),
        identity: crate::reopen::Identity {
            id: id.map(str::to_owned),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn a_chat_on_a_harness_is_known_by_its_ulid_its_harness_and_its_persona() {
    let plane = plane_with(vec![
        chat(3, Some("claude"), Some("steward"), Some(CHAT)),
        chat(4, Some("codex"), None, None),
    ]);
    assert_eq!(
        Provenance::of_chat(plane.path(), 3),
        Some(Provenance {
            harness: Some("claude-code".into()),
            chat: Some(CHAT.into()),
            persona: Some("steward".into()),
            ..Default::default()
        })
    );
    assert_eq!(
        Provenance::of_chat(plane.path(), 4),
        Some(Provenance {
            harness: Some("codex".into()),
            ..Default::default()
        }),
        "a chat recorded before ids, with no persona"
    );
}

#[test]
fn a_chat_whose_harness_reported_its_model_says_harness_colon_model() {
    // #1021 (from #1009): once the harness has said which model, `Assisted-by` names it; a
    // chat it has not said it for keeps the harness alone.
    let plane = plane_with(vec![
        crate::reopen::Chat {
            model: Some("claude-opus-4-1".into()),
            ..chat(3, Some("claude"), Some("steward"), Some(CHAT))
        },
        chat(4, Some("claude"), None, None),
    ]);
    let said = Provenance::of_chat(plane.path(), 3).expect("an agent run");
    assert_eq!(said.model.as_deref(), Some("claude-opus-4-1"));
    assert_eq!(
        said.trailers(Form::Full)[0],
        "Assisted-by: claude-code:claude-opus-4-1"
    );
    let unsaid = Provenance::of_chat(plane.path(), 4).expect("an agent run");
    assert_eq!(unsaid.trailers(Form::Full)[0], "Assisted-by: claude-code");
    assert_eq!(said.trailers(Form::Llm)[0], "Assisted-by: LLM");
}

#[test]
fn a_shell_a_chat_the_record_does_not_hold_and_no_record_are_no_agent_run() {
    let plane = plane_with(vec![chat(5, None, Some("steward"), Some(CHAT))]);
    assert_eq!(
        Provenance::of_chat(plane.path(), 5),
        None,
        "a shell tab is the operator's"
    );
    assert_eq!(Provenance::of_chat(plane.path(), 6), None);
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(Provenance::of_chat(empty.path(), 5), None);
}

/// A project with workspace `ws` holding a clone `api` on `branch`, and change `billing-v2`
/// whose `api` member is on `change/billing-v2`.
fn project_with_a_change(branch: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let plane = dir.path();
    let clone = plane.join("workspaces").join("ws").join("api");
    std::fs::create_dir_all(&clone).unwrap();
    crate::testgit::run(&clone, &["init", "-q", "-b", branch]);
    let mut record =
        crate::change::Record::new("billing-v2", "one bill", "steward", "2026-10-02T00:00:00Z");
    record.members.push(crate::change::Member {
        repo: "api".into(),
        branch: "change/billing-v2".into(),
        needs: Vec::new(),
    });
    crate::change::store::write(plane, "ws", &record).unwrap();
    (dir, clone)
}

#[test]
fn a_commit_on_a_change_members_branch_names_the_change() {
    let (plane, clone) = project_with_a_change("change/billing-v2");
    assert_eq!(
        change_of(plane.path(), &clone).as_deref(),
        Some("billing-v2")
    );
}

#[test]
fn a_commit_on_any_other_branch_or_outside_a_workspace_names_no_change() {
    let (plane, clone) = project_with_a_change("main");
    assert_eq!(change_of(plane.path(), &clone), None);
    assert_eq!(
        change_of(plane.path(), plane.path()),
        None,
        "the project itself"
    );
}

#[test]
fn a_process_inside_a_chat_finds_its_chat_by_the_number_the_app_set() {
    let plane = plane_with(vec![crate::reopen::Chat {
        pid: Some(std::process::id()),
        ..chat(3, Some("claude"), Some("steward"), Some(CHAT))
    }]);
    let env = |n: &str| (n == "PURLIS_SESSION_ID").then(|| "3".to_owned());
    assert_eq!(
        Provenance::in_chat(plane.path(), &env).and_then(|p| p.chat),
        Some(CHAT.into())
    );
    let outside = |_: &str| None;
    assert_eq!(
        Provenance::in_chat(plane.path(), &outside),
        None,
        "a terminal of the operator's"
    );
    let not_a_number = |n: &str| (n == "PURLIS_SESSION_ID").then(|| "abc".to_owned());
    assert_eq!(Provenance::in_chat(plane.path(), &not_a_number), None);
}

/// A project whose chat 3 runs claude as the steward, holding workspace `ws`'s clone `api` on
/// the change's branch, and the message git hands `commit-msg`.
struct Commit {
    plane: tempfile::TempDir,
    clone: std::path::PathBuf,
    message: std::path::PathBuf,
}

impl Commit {
    fn new(message: &str) -> Self {
        let (plane, clone) = project_with_a_change("change/billing-v2");
        let mut record = crate::reopen::Record {
            chats: vec![chat(3, Some("claude"), Some("steward"), Some(CHAT))],
            dealt: 3,
            ..Default::default()
        };
        record.chats[0].cwd = Some(clone.clone());
        // This test process stands in for the chat's harness: `stamp` runs inside it.
        record.chats[0].pid = Some(std::process::id());
        crate::reopen::write(plane.path(), &record).unwrap();
        let message_file = clone.join(".git").join("COMMIT_EDITMSG");
        std::fs::write(&message_file, message).unwrap();
        Self {
            plane,
            clone,
            message: message_file,
        }
    }

    fn env(&self) -> impl Fn(&str) -> Option<String> + '_ {
        |name| match name {
            "PURLIS_ROOT" => Some(self.plane.path().display().to_string()),
            "PURLIS_SESSION_ID" => Some("3".into()),
            _ => None,
        }
    }

    fn stamped(&self) -> String {
        std::fs::read_to_string(&self.message).unwrap()
    }
}

#[test]
fn an_agents_own_commit_in_a_workspace_repo_is_stamped_with_all_four() {
    let commit = Commit::new("fix: one bill\n");
    stamp(&commit.message, &commit.clone, &commit.env());
    assert_eq!(
        commit.stamped(),
        "fix: one bill\n\n\
         Assisted-by: claude-code\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n\
         Purlis-Persona: steward\n\
         Purlis-Change: billing-v2\n"
    );
}

#[test]
fn a_message_stamped_twice_an_amend_carries_each_trailer_once_after_the_operators_own() {
    let commit =
        Commit::new("fix: one bill\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n");
    stamp(&commit.message, &commit.clone, &commit.env());
    stamp(&commit.message, &commit.clone, &commit.env());
    assert_eq!(
        commit.stamped(),
        "fix: one bill\n\n\
         Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\
         Assisted-by: claude-code\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n\
         Purlis-Persona: steward\n\
         Purlis-Change: billing-v2\n"
    );
}

#[test]
fn an_amend_of_a_commit_made_before_the_rename_is_not_given_purlis_twins_of_its_trailers() {
    // History keeps `Charter-*` forever (V93j): the same claim under the old key is had.
    let old = "fix: one bill\n\n\
               Assisted-by: claude-code\n\
               Charter-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n\
               Charter-Persona: steward\n\
               Charter-Change: billing-v2\n";
    let commit = Commit::new(old);
    stamp(&commit.message, &commit.clone, &commit.env());
    assert_eq!(commit.stamped(), old);
}

#[test]
fn an_old_trailer_with_another_value_is_a_different_claim_and_the_purlis_one_is_added() {
    let message = "fix\n\nCharter-Chat: 01HZZZZZZZZZZZZZZZZZZZZZZZ\n";
    let trailers = everything().trailers(Form::Full);
    let stamped = append(message, &trailers, None);
    assert!(
        stamped.contains("\nPurlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n"),
        "{stamped}"
    );
    assert!(stamped.contains("\nPurlis-Persona: steward\n"), "{stamped}");
}

#[test]
fn a_repo_that_follows_the_kernel_is_stamped_assisted_by_llm() {
    let commit = Commit::new("fix: one bill\n");
    std::fs::write(
        commit.plane.path().join("charter.toml"),
        "[repos.api]\nassisted_by = \"llm\"\n",
    )
    .unwrap();
    stamp(&commit.message, &commit.clone, &commit.env());
    assert!(
        commit
            .stamped()
            .contains("\nAssisted-by: LLM\nPurlis-Chat: "),
        "{}",
        commit.stamped()
    );
}

#[test]
fn a_commit_outside_a_chat_is_left_as_it_was_written() {
    let commit = Commit::new("fix: one bill\n");
    let operator =
        |name: &str| (name == "PURLIS_ROOT").then(|| commit.plane.path().display().to_string());
    stamp(&commit.message, &commit.clone, &operator);
    assert_eq!(commit.stamped(), "fix: one bill\n");
}

/// Chat 3's recorded harness made `pid`, as the app records it at each start.
fn harness_is(commit: &Commit, pid: Option<u32>) {
    let mut record = crate::reopen::read_or_refusal(commit.plane.path()).unwrap();
    record.chats[0].pid = pid;
    crate::reopen::write(commit.plane.path(), &record).unwrap();
}

/// A live process that is not this one and not any of its ancestors: an editor the chat
/// started, which went on to run on its own.
fn elsewhere() -> std::process::Child {
    crate::forklock::spawn(std::process::Command::new("sleep").arg("30")).unwrap()
}

#[test]
fn a_process_with_the_chats_environment_outside_its_harnesss_tree_stamps_nothing() {
    // V82 (#1018): an IDE opened from a chat keeps the chat's environment, and the operator's
    // commits there are theirs.
    let commit = Commit::new("fix: by hand\n");
    let mut editor = elsewhere();
    harness_is(&commit, Some(editor.id()));

    stamp(&commit.message, &commit.clone, &commit.env());

    let _ = editor.kill();
    let _ = editor.wait();
    assert_eq!(commit.stamped(), "fix: by hand\n");
}

#[test]
fn a_chat_the_record_names_no_harness_process_for_stamps_nothing() {
    let commit = Commit::new("fix: by hand\n");
    harness_is(&commit, None);

    stamp(&commit.message, &commit.clone, &commit.env());

    assert_eq!(commit.stamped(), "fix: by hand\n");
}

#[test]
fn the_harness_word_is_v67s_and_comes_from_the_chats_profile_not_its_program() {
    let plane = plane_with(vec![
        crate::reopen::Chat {
            program: "/usr/local/bin/claude".into(),
            ..chat(3, Some("work"), None, Some(CHAT))
        },
        chat(4, Some("opencode"), None, None),
        chat(5, Some("gone"), None, None),
    ]);
    std::fs::write(
        plane.path().join("charter.local.toml"),
        "[harness.work]\nkind = \"codex\"\ncommand = [\"codex\"]\n",
    )
    .unwrap();
    let harness = |n| Provenance::of_chat(plane.path(), n).and_then(|p| p.harness);
    assert_eq!(
        harness(3).as_deref(),
        Some("codex"),
        "the profile's kind, whatever runs"
    );
    assert_eq!(harness(4).as_deref(), Some("opencode"));
    assert_eq!(harness(5), None, "a profile this project no longer has");
}

#[test]
fn a_value_up_to_a_hundred_characters_is_kept_and_a_longer_one_left_out() {
    // The bound is on a trailer's whole value. For `Assisted-by` that value is
    // `<harness>:<model>`, so the model is sized to make the whole value 100, then 101.
    let assisted_by = |model_len: usize| {
        Provenance {
            model: Some("m".repeat(model_len)),
            ..everything()
        }
        .trailers(Form::Full)[0]
            .clone()
    };
    let room = MOST - "claude-code:".len();
    let kept = assisted_by(room);
    let value = kept.strip_prefix("Assisted-by: ").expect("the key");
    assert_eq!(value.len(), 100, "{kept}");
    assert_eq!(
        kept,
        format!("Assisted-by: claude-code:{}", "m".repeat(room))
    );
    assert_eq!(assisted_by(room + 1), "Assisted-by: claude-code");

    // A value that stands alone is held to the same 100.
    let persona = |len: usize| {
        Provenance {
            persona: Some("p".repeat(len)),
            ..everything()
        }
        .trailers(Form::Full)
        .into_iter()
        .find(|line| line.starts_with(&format!("{PERSONA}: ")))
    };
    assert_eq!(
        persona(100),
        Some(format!("{PERSONA}: {}", "p".repeat(100)))
    );
    assert_eq!(persona(101), None);

    // And the reader keeps what the writer keeps, and leaves out what it leaves out.
    assert_eq!(
        Claims::read(&kept).assisted_by.one(),
        Some(value),
        "a whole value of 100 reads back"
    );
    let over = format!("Assisted-by: claude-code:{}", "m".repeat(room + 1));
    assert_eq!(Claims::read(&over).assisted_by, Claim::Unsaid);
}

fn lines(trailers: &[&str]) -> Vec<String> {
    trailers.iter().map(|t| (*t).to_owned()).collect()
}

const OURS: [&str; 2] = [
    "Assisted-by: claude-code",
    "Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K",
];

#[test]
fn a_message_with_no_trailers_gets_a_blank_line_and_then_the_block() {
    assert_eq!(
        append("fix: one bill\n", &lines(&OURS), None),
        "fix: one bill\n\nAssisted-by: claude-code\nPurlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n"
    );
    assert_eq!(
        append("fix: one bill", &lines(&OURS), None),
        "fix: one bill\n\nAssisted-by: claude-code\nPurlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n",
        "no newline at the end"
    );
}

#[test]
fn the_agents_own_lines_stay_byte_for_byte_and_ours_join_its_trailer_block() {
    assert_eq!(
        append("fix\n\nbody\n\nCo-authored-by:x\n", &lines(&OURS), None),
        "fix\n\nbody\n\nCo-authored-by:x\nAssisted-by: claude-code\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n"
    );
}

#[test]
fn a_bare_url_after_refs_is_left_exactly_as_written() {
    assert_eq!(
        append(
            "fix\n\nRefs:\nhttps://example.com/a?b=c\n",
            &lines(&OURS),
            None
        ),
        "fix\n\nRefs:\nhttps://example.com/a?b=c\nAssisted-by: claude-code\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n"
    );
}

#[test]
fn a_dashed_line_in_the_body_is_not_a_patch_divider() {
    assert_eq!(
        append("fix\n\nbefore\n---\nafter\n", &lines(&OURS), None),
        "fix\n\nbefore\n---\nafter\n\nAssisted-by: claude-code\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n"
    );
}

#[test]
fn a_line_already_there_is_not_added_again_and_a_message_with_all_of_them_is_untouched() {
    let half = "fix\n\nAssisted-by: claude-code\n";
    assert_eq!(
        append(half, &lines(&OURS), None),
        "fix\n\nAssisted-by: claude-code\nPurlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n"
    );
    let whole = "fix\n\nAssisted-by: claude-code\nPurlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K";
    assert_eq!(append(whole, &lines(&OURS), None), whole);
}

#[test]
fn in_an_edited_message_ours_go_before_the_comments_and_the_scissors() {
    let edited = "fix\n\nSigned-off-by: A <a@example.invalid>\n\n# Please enter the message.\n\
                  # ------------------------ >8 ------------------------\ndiff --git a/x b/x\n";
    assert_eq!(
        append(edited, &lines(&OURS), Some("#")),
        "fix\n\nSigned-off-by: A <a@example.invalid>\nAssisted-by: claude-code\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n\n# Please enter the message.\n\
         # ------------------------ >8 ------------------------\ndiff --git a/x b/x\n"
    );
}

#[test]
fn an_empty_message_is_left_empty_for_git_to_refuse() {
    assert_eq!(append("", &lines(&OURS), None), "");
    assert_eq!(
        append("\n# only a comment\n", &lines(&OURS), Some("#")),
        "\n# only a comment\n"
    );
}

#[test]
fn a_crlf_message_gets_its_trailers_with_crlf_endings() {
    assert_eq!(
        append("fix: one bill\r\n", &lines(&OURS), None),
        "fix: one bill\r\n\r\nAssisted-by: claude-code\r\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\r\n"
    );
    assert_eq!(
        append(
            "fix\r\n\r\nbody\r\n\r\nCo-authored-by: x",
            &lines(&OURS),
            None
        ),
        "fix\r\n\r\nbody\r\n\r\nCo-authored-by: x\r\nAssisted-by: claude-code\r\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\r\n",
        "no line ending at the end: the message's own"
    );
    assert_eq!(
        append(
            "fix\r\n\r\n# Please enter the message.\r\n",
            &lines(&OURS),
            Some("#")
        ),
        "fix\r\n\r\nAssisted-by: claude-code\r\nPurlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\r\n\
         \r\n# Please enter the message.\r\n"
    );
}

#[test]
fn a_message_no_editor_saw_keeps_its_hash_lines_as_text() {
    let none = |name: &str| (name == "GIT_EDITOR").then(|| ":".to_owned());
    assert_eq!(comment_for(Path::new("/nonexistent"), &none), None);
    assert_eq!(
        append("fix\n\n# not a comment under -m\n", &lines(&OURS), None),
        "fix\n\n# not a comment under -m\n\nAssisted-by: claude-code\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n"
    );
}

#[test]
fn a_dash_m_message_ending_in_a_hash_line_gets_the_trailers_after_it() {
    let commit = Commit::new("fix: one bill\n#123 is the ticket\n");
    let env = commit.env();
    let no_editor = |name: &str| match name {
        "GIT_EDITOR" => Some(":".to_owned()),
        other => env(other),
    };
    stamp(&commit.message, &commit.clone, &no_editor);
    assert_eq!(
        commit.stamped(),
        "fix: one bill\n#123 is the ticket\n\n\
         Assisted-by: claude-code\n\
         Purlis-Chat: 01J9ZQ3W5Y7X8V6T4R2P0N1M3K\n\
         Purlis-Persona: steward\n\
         Purlis-Change: billing-v2\n"
    );
}

// ----- reading them back (#1019) -----

#[test]
fn a_commit_with_one_value_per_key_claims_each() {
    let claims = Claims::read(&format!(
        "Co-authored-by: Someone <s@example.com>\nAssisted-by: claude-code:claude-opus-4-1\n\
         Purlis-Chat: {CHAT}\nPurlis-Persona: steward\nPurlis-Change: billing-v2\n"
    ));
    assert_eq!(
        claims.assisted_by.one(),
        Some("claude-code:claude-opus-4-1")
    );
    assert_eq!(claims.chat.one(), Some(CHAT));
    assert_eq!(claims.persona.one(), Some("steward"));
    assert_eq!(claims.change.one(), Some("billing-v2"));
    assert_eq!(Claims::read(""), Claims::default());
}

#[test]
fn the_same_value_under_either_spelling_or_any_case_is_one_claim() {
    let claims = Claims::read(&format!(
        "Charter-Chat: {CHAT}\nPurlis-Chat: {CHAT}\npurlis-chat: {CHAT}\n"
    ));
    assert_eq!(claims.chat, Claim::One(CHAT.into()));
}

#[test]
fn several_values_for_a_key_leave_that_key_unknown_whichever_comes_last() {
    // A cherry-pick from another chat, then purlis's own line: neither is the commit's.
    const OTHER: &str = "01J9ZQ3W5Y7X8V6T4R2P0N1M3M";
    let picked = Claims::read(&format!(
        "Charter-Chat: {OTHER}\nPurlis-Persona: steward\n\
         Purlis-Chat: {CHAT}\nPurlis-Chat: {OTHER}\n"
    ));
    assert_eq!(picked.chat, Claim::Several(vec![OTHER.into(), CHAT.into()]));
    assert_eq!(picked.chat.one(), None);
    // Only the key with several is unknown.
    assert_eq!(picked.persona.one(), Some("steward"));
}

#[test]
fn a_value_that_could_not_be_written_claims_nothing() {
    let claims = Claims::read(&format!(
        "Purlis-Chat: two words\nPurlis-Chat:\nPurlis-Chat: {CHAT}\nNot a trailer\n"
    ));
    assert_eq!(claims.chat, Claim::One(CHAT.into()));
}

#[test]
fn what_append_writes_reads_back_as_what_was_stamped() {
    let message = append(
        "Fix it\n\nBody.\n",
        &everything().trailers(Form::Full),
        None,
    );
    let block = message.split("\n\n").last().expect("a block");
    let claims = Claims::read(block);
    let p = everything();
    assert_eq!(
        claims.assisted_by.one(),
        Some(format!("{}:{}", p.harness.unwrap(), p.model.unwrap()).as_str())
    );
    assert_eq!(claims.chat.one(), p.chat.as_deref());
    assert_eq!(claims.persona.one(), p.persona.as_deref());
    assert_eq!(claims.change.one(), p.change.as_deref());
}

#[test]
fn a_model_as_long_as_one_is_kept_still_fits_beside_every_harness_and_reads_back() {
    // #1021 review: the trailer's value is `<harness>:<model>`, and its reader drops a value
    // over the bound, so the longest model kept must fit beside the longest harness word.
    for kind in ["claude", "codex", "opencode"] {
        let harness = harness_word(kind).expect("a harness word");
        assert!(
            harness.len() + 1 + crate::state::Model::MOST <= MOST,
            "{harness}"
        );
    }
    let longest = "m".repeat(crate::state::Model::MOST);
    assert!(crate::state::Model::new(&longest).is_some());
    assert!(crate::state::Model::new(&format!("{longest}m")).is_none());

    let said = Provenance {
        harness: Some("claude-code".into()),
        model: Some(longest.clone()),
        ..Provenance::default()
    };
    let line = said.trailers(Form::Full).remove(0);
    assert_eq!(line, format!("Assisted-by: claude-code:{longest}"));
    assert_eq!(
        Claims::read(&line).assisted_by.one(),
        Some(format!("claude-code:{longest}").as_str())
    );

    // A model that would not fit beside its harness, however it got here, leaves the harness
    // alone rather than a line its reader would pass over.
    let too_long = Provenance {
        model: Some(format!("{longest}m")),
        ..said
    };
    assert_eq!(too_long.trailers(Form::Full)[0], "Assisted-by: claude-code");
}
