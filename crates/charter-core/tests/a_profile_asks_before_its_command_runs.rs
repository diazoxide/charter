//! Whether a profile's command may run without asking.
//!
//! A profile's command runs on a click, and `charter.local.toml` is gitignored — so an edit
//! to it leaves no diff for a reviewer to catch, and nothing stops a chat editing plane
//! config. So charter records what it last ran each profile as, and a profile with no
//! record, or with a different one, is shown its command and asked before it runs.
//!
//! **Every unreadable state means ask again.** A missing file, a malformed one, an entry
//! that is not a fingerprint: each reads as "no record", never as approval. Treating
//! silence as a yes is the one state this record exists to keep out.

use std::fs;

use charter_core::profiles::{self, Profile, Source};
use charter_core::profiletrust::{self, Approval};

fn plane(local: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), "").unwrap();
    fs::write(dir.path().join(profiles::LOCAL_FILE), local).unwrap();
    dir
}

const WORK: &str = "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
                    env = { CLAUDE_CONFIG_DIR = \"~/.claude-work\" }\n";

fn work(root: &std::path::Path) -> Profile {
    profiles::current(root).get("work").unwrap().clone()
}

#[test]
fn a_declared_profile_nothing_has_recorded_asks_because_it_is_new() {
    let dir = plane(WORK);

    assert_eq!(
        profiletrust::approval_needed(dir.path(), &work(dir.path())),
        Some(Approval::New)
    );
}

#[test]
fn a_built_in_never_asks_because_its_command_is_charters_own() {
    // Its command comes out of charter's registry rather than out of a file, so what a chat
    // could have written decides nothing about it — and a question that never carries risk
    // is one an operator learns to answer yes to without reading.
    let dir = plane("");
    let builtin = profiles::current(dir.path()).get("claude").unwrap().clone();
    assert_eq!(builtin.source, Source::BuiltIn);

    assert_eq!(profiletrust::approval_needed(dir.path(), &builtin), None);
}

#[test]
fn a_profile_declared_with_a_built_ins_name_asks_with_the_rest() {
    // The name is the built-in's and the command is the file's, so it is a declaration.
    let dir = plane("[harness.claude]\nkind = \"claude\"\ncommand = [\"~/bin/claude\"]\n");
    let declared = profiles::current(dir.path()).get("claude").unwrap().clone();

    assert_eq!(
        profiletrust::approval_needed(dir.path(), &declared),
        Some(Approval::New)
    );
}

#[test]
fn a_profile_recorded_exactly_as_it_is_now_runs_without_asking_again() {
    let dir = plane(WORK);
    let p = work(dir.path());

    profiletrust::record_launched(dir.path(), &p.name, &profiletrust::fingerprint(&p)).unwrap();

    assert_eq!(profiletrust::approval_needed(dir.path(), &p), None);
}

#[test]
fn a_command_that_changed_since_it_was_approved_asks_again() {
    let dir = plane(WORK);
    let p = work(dir.path());
    profiletrust::record_launched(dir.path(), &p.name, &profiletrust::fingerprint(&p)).unwrap();

    fs::write(
        dir.path().join(profiles::LOCAL_FILE),
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\", \"--dangerously-skip-permissions\"]\n\
         env = { CLAUDE_CONFIG_DIR = \"~/.claude-work\" }\n",
    )
    .unwrap();

    assert_eq!(
        profiletrust::approval_needed(dir.path(), &work(dir.path())),
        Some(Approval::Changed)
    );
}

#[test]
fn an_environment_that_changed_since_it_was_approved_asks_again() {
    // The variable is how a second account is selected in the first place, so a change to
    // it is a change to which account the click reaches.
    let dir = plane(WORK);
    let p = work(dir.path());
    profiletrust::record_launched(dir.path(), &p.name, &profiletrust::fingerprint(&p)).unwrap();

    fs::write(
        dir.path().join(profiles::LOCAL_FILE),
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
         env = { CLAUDE_CONFIG_DIR = \"~/.claude-other\" }\n",
    )
    .unwrap();

    assert_eq!(
        profiletrust::approval_needed(dir.path(), &work(dir.path())),
        Some(Approval::Changed)
    );
}

#[test]
fn a_record_that_cannot_be_read_asks_again_rather_than_letting_the_command_run() {
    let dir = plane(WORK);
    let p = work(dir.path());
    profiletrust::record_launched(dir.path(), &p.name, &profiletrust::fingerprint(&p)).unwrap();
    fs::write(
        dir.path().join(".charter").join(profiletrust::RECORD),
        "{ this is not json",
    )
    .unwrap();

    assert_eq!(
        profiletrust::approval_needed(dir.path(), &p),
        Some(Approval::New),
        "a record charter could not read was taken as approval"
    );
}

#[test]
fn what_is_recorded_is_the_tilde_as_written_and_never_where_it_points() {
    // A home directory that moved would otherwise make every profile read as CHANGED and
    // ask again about a command nobody touched.
    let dir = plane(WORK);

    let print = profiletrust::fingerprint(&work(dir.path()));

    assert_eq!(
        print.env.get("CLAUDE_CONFIG_DIR").map(String::as_str),
        Some("~/.claude-work")
    );
}
