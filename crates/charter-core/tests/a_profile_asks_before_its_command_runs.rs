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
    charter_core::unsteered!();
    let dir = plane(WORK);

    assert_eq!(
        profiletrust::approval_needed(dir.path(), &work(dir.path())),
        Some(Approval::New)
    );
}

#[test]
fn a_built_in_never_asks_because_its_command_is_charters_own() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
    let dir = plane(WORK);
    let p = work(dir.path());

    profiletrust::record_launched(dir.path(), &p.name, &profiletrust::fingerprint(&p)).unwrap();

    assert_eq!(profiletrust::approval_needed(dir.path(), &p), None);
}

#[test]
fn a_command_that_changed_since_it_was_approved_asks_again() {
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
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
    charter_core::unsteered!();
    // A home directory that moved would otherwise make every profile read as CHANGED and
    // ask again about a command nobody touched.
    let dir = plane(WORK);

    let print = profiletrust::fingerprint(&work(dir.path()));

    assert_eq!(
        print.env.get("CLAUDE_CONFIG_DIR").map(String::as_str),
        Some("~/.claude-work")
    );
}

// --------------------------------------------------------------------------------------
// the record as an OBJECT, not only as JSON (M3)
//
// Everything above drives the record's CONTENTS. These drive what the record IS — a link,
// a FIFO, a giant — which is the half `read` had no answer for at all: it was a bare
// `read_to_string` on the one file in this crate that decides whether a command line is
// shown to a person before it runs, while its own writer four lines down is contained.
// --------------------------------------------------------------------------------------

/// The record's path under `root`.
fn record(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".charter").join(profiletrust::RECORD)
}

#[cfg(unix)]
#[test]
fn a_record_that_is_a_link_out_of_the_plane_is_not_this_planes_consent() {
    charter_core::unsteered!();
    // An approval is per PLANE. A `.charter/harness-profiles-launched.json` pointing at a
    // file somewhere else answers with somebody else's yes — and that answer is what skips
    // the question in front of a command out of a gitignored file.
    let dir = plane(WORK);
    let p = work(dir.path());
    let elsewhere = tempfile::tempdir().unwrap();
    let theirs = elsewhere.path().join("approved.json");
    // Read THROUGH the link this is an exact match, so what the assertion is about is the
    // link and not the fingerprint.
    fs::write(
        &theirs,
        "{\"work\": {\"kind\": \"claude\", \"command\": [\"claude\"], \
         \"env\": {\"CLAUDE_CONFIG_DIR\": \"~/.claude-work\"}}}",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join(".charter")).unwrap();
    std::os::unix::fs::symlink(&theirs, record(dir.path())).unwrap();

    assert_eq!(
        profiletrust::last_launched(dir.path(), "work"),
        None,
        "a record reached through a link was read as this plane's"
    );
    assert_eq!(
        profiletrust::approval_needed(dir.path(), &p),
        Some(Approval::New)
    );
}

#[cfg(unix)]
#[test]
fn a_fifo_where_the_record_goes_asks_again_instead_of_never_returning() {
    charter_core::unsteered!();
    // `approval_needed` runs on the app's startup path, so a read that blocks is an app
    // with no window and no tray, killable only from a terminal. The same hazard, and the
    // same answer, as the launch record ADR 0028 fixed one file over.
    //
    // On its own thread with a deadline, because a test that simply called this would HANG
    // rather than fail if the guard went away — and a hung test says nothing.
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = plane(WORK);
    let p = work(dir.path());
    fs::create_dir_all(dir.path().join(".charter")).unwrap();
    let made = charter_core::forklock::status(
        std::process::Command::new("mkfifo").arg(record(dir.path())),
    )
    .expect("mkfifo runs");
    assert!(made.success(), "the test's own premise: a FIFO was made");

    let (tx, rx) = mpsc::channel();
    let root = dir.path().to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(profiletrust::approval_needed(&root, &p));
    });

    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Some(Approval::New)),
        "the read of a FIFO never returned"
    );
}

#[test]
fn a_record_larger_than_the_bound_asks_again_rather_than_being_read_whole() {
    charter_core::unsteered!();
    let dir = plane(WORK);
    let p = work(dir.path());
    profiletrust::record_launched(dir.path(), &p.name, &profiletrust::fingerprint(&p)).unwrap();
    assert_eq!(
        profiletrust::approval_needed(dir.path(), &p),
        None,
        "the premise: recorded exactly as it is, this profile does not ask"
    );

    let mut giant = String::from("{\"pad\": \"");
    giant.push_str(&"p".repeat(2 * 1024 * 1024));
    giant.push_str(
        "\", \"work\": {\"kind\": \"claude\", \"command\": [\"claude\"], \
                    \"env\": {\"CLAUDE_CONFIG_DIR\": \"~/.claude-work\"}}}",
    );
    fs::write(record(dir.path()), giant).unwrap();

    assert_eq!(
        profiletrust::approval_needed(dir.path(), &p),
        Some(Approval::New)
    );
}
