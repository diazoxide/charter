//! A chat that started on a profile comes back on the same profile and the same persona.
//!
//! The record stores the profile's NAME, not its command or its environment, and the reopen
//! re-reads `charter.local.toml`. That is ADR 0022's rule and it is load-bearing twice: an
//! edited profile takes effect at the next launch rather than a stale copy running, and a
//! profile that is GONE means the chat is skipped by name rather than started on something
//! else's account.
//!
//! It also keeps the account out of the record. A profile's environment is where a second
//! Claude Code config folder is named, and none of it is written to a file that survives
//! the app.

use std::fs;

use charter_core::reopen::{self, Chat, Record};

fn a_chat_on(profile: &str, persona: Option<&str>) -> Chat {
    Chat {
        program: "claude".to_owned(),
        args: Vec::new(),
        cwd: None,
        name: "ide.7".to_owned(),
        resume: None,
        active: true,
        profile: Some(profile.to_owned()),
        persona: persona.map(str::to_owned),
    }
}

#[test]
fn the_profile_and_the_persona_survive_a_quit_and_come_back() {
    let dir = tempfile::tempdir().unwrap();
    let record = Record {
        chats: vec![a_chat_on("claude-work", Some("steward"))],
    };

    reopen::write(dir.path(), &record).unwrap();
    let back = reopen::read_or_refusal(dir.path()).unwrap();

    assert_eq!(back.chats[0].profile.as_deref(), Some("claude-work"));
    assert_eq!(back.chats[0].persona.as_deref(), Some("steward"));
}

#[test]
fn the_record_never_holds_the_environment_that_names_an_account() {
    // A profile's `env` is where a second config folder is named. Writing it down would put
    // the account into a file that outlives the app and travels with the plane's state
    // directory — and it would freeze it, so an edit to the profile would not take.
    let dir = tempfile::tempdir().unwrap();
    reopen::write(
        dir.path(),
        &Record {
            chats: vec![a_chat_on("claude-work", None)],
        },
    )
    .unwrap();

    let text = fs::read_to_string(dir.path().join(reopen::IN_PLANE)).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).expect("the record is JSON");
    let chat = &doc["chats"][0];

    assert_eq!(chat["profile"], "claude-work");
    // Asked of the KEYS rather than of the text, so this keeps meaning what it says if the
    // record is ever written differently.
    let keys: Vec<&str> = chat
        .as_object()
        .expect("a chat is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert!(
        !keys.contains(&"env"),
        "the record grew an environment: {keys:?}"
    );
    assert!(!text.contains("CLAUDE_CONFIG_DIR"), "{text}");
}

#[test]
fn a_record_written_before_profiles_existed_still_reads_as_a_chat_with_none() {
    // The file is written by a process that may be an older version. A chat with no profile
    // is the shell the app opened before there was a picker, and it still comes back.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".charter/app")).unwrap();
    fs::write(
        dir.path().join(reopen::IN_PLANE),
        r#"{"version":1,"at":1789000000,"chats":[{"program":"/bin/zsh","args":[],"cwd":"","name":"1","resume":"","active":true}]}"#,
    )
    .unwrap();

    let back = reopen::read_or_refusal(dir.path()).unwrap();

    assert_eq!(back.chats[0].profile, None);
    assert_eq!(back.chats[0].persona, None);
    assert_eq!(back.chats[0].program, "/bin/zsh");
}

#[test]
fn a_profile_name_that_is_not_one_charter_would_mint_reads_as_no_profile() {
    // The name goes back to `profiles::current` as a lookup key and never onto a command
    // line, but it is also printed in a sidebar and in a refusal. A value off disk that is
    // not a name charter accepts is not one this app will carry.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".charter/app")).unwrap();
    fs::write(
        dir.path().join(reopen::IN_PLANE),
        r#"{"version":1,"at":1789000000,"chats":[{"program":"claude","args":[],"cwd":"","name":"1","resume":"","active":true,"profile":"../../etc/passwd","persona":"Not A Persona"}]}"#,
    )
    .unwrap();

    let back = reopen::read_or_refusal(dir.path()).unwrap();

    assert_eq!(
        back.chats[0].profile, None,
        "a traversing profile name was kept"
    );
    assert_eq!(
        back.chats[0].persona, None,
        "a name no persona has was kept"
    );
}
