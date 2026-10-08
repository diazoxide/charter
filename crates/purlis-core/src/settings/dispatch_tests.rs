//! The project's dispatch grants in the committed file (#1437): what Allow for everyone and
//! Revoke write, and that each is read back.

use std::fs;

use super::*;
use crate::dispatchgrant::{committed, committed_at};

fn pair(asking: &str, target: &str) -> Pair {
    Pair::new(asking, target).expect("a pair")
}

const PROJECT: &str = "schema = 1 # ours\n\n[sandbox]\nmode = \"on\"\n";

// ---- the text, which a test can check anywhere --------------------------------------------------

#[test]
fn a_grant_is_written_under_dispatch_grants_keeping_every_other_line_and_reads_back() {
    let after = with(PROJECT, &pair("steward", "devops")).expect("written");
    assert_eq!(
        after,
        "schema = 1 # ours\n\n[sandbox]\nmode = \"on\"\n\n[dispatch.grants]\nsteward = \
         [\"devops\"]\n"
    );
    assert_eq!(committed(Some(&after)).pairs, [pair("steward", "devops")]);
}

#[test]
fn a_second_target_joins_its_persona_s_list_and_one_granted_already_changes_nothing() {
    let one = with(PROJECT, &pair("steward", "devops")).expect("written");
    let two = with(&one, &pair("steward", "qa")).expect("written");
    assert!(
        two.ends_with("[dispatch.grants]\nsteward = [\"devops\", \"qa\"]\n"),
        "{two}"
    );
    assert_eq!(with(&two, &pair("steward", "qa")).expect("the same"), two);
    let three = with(&two, &pair("qa", "devops")).expect("written");
    assert_eq!(
        committed(Some(&three)).pairs,
        [
            pair("steward", "devops"),
            pair("steward", "qa"),
            pair("qa", "devops")
        ]
    );
}

#[test]
fn a_revoke_takes_the_pair_out_and_the_table_with_its_last_one() {
    let one = with(PROJECT, &pair("steward", "devops")).expect("written");
    let two = with(&one, &pair("steward", "qa")).expect("written");
    let less = without(&two, &pair("steward", "devops")).expect("written");
    assert_eq!(committed(Some(&less)).pairs, [pair("steward", "qa")]);
    let none = without(&less, &pair("steward", "qa")).expect("written");
    assert_eq!(none, PROJECT);
    // One that was never granted changes nothing.
    assert_eq!(
        without(PROJECT, &pair("steward", "devops")).expect("same"),
        PROJECT
    );
}

#[test]
fn a_revoke_keeps_what_else_dispatch_holds() {
    let text = "[dispatch]\nrunning = 6\n\n[dispatch.grants]\nsteward = [\"devops\"]\n";
    assert_eq!(
        without(text, &pair("steward", "devops")).expect("written"),
        "[dispatch]\nrunning = 6\n"
    );
}

#[test]
fn a_file_that_is_not_toml_or_writes_grants_another_way_is_refused_unchanged() {
    let why = with("not toml [", &pair("steward", "devops")).expect_err("refused");
    assert!(why.contains("is not valid TOML"), "{why}");
    for text in [
        "dispatch = 3\n",
        "[dispatch]\ngrants = [\"steward\"]\n",
        "[dispatch.grants]\nsteward = \"devops\"\n",
    ] {
        let why = with(text, &pair("steward", "devops")).expect_err("refused");
        assert!(
            why.ends_with(
                "is not written in a form purlis edits, so nothing was changed. Change it \
                 under Edit as TOML."
            ),
            "{text}: {why}"
        );
    }
}

// ---- the file, at a project ---------------------------------------------------------------------

/// A project with a committed file. Writing a project file is refused inside a sandboxed chat,
/// so these run first in CI.
fn project(shared: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a project");
    fs::write(crate::names::manifest(dir.path()), shared).expect("the project file");
    dir
}

#[test]
fn allow_for_everyone_is_kept_in_the_committed_file_and_is_no_news_to_whoever_made_it() {
    let dir = project(PROJECT);
    grant(dir.path(), &pair("steward", "devops")).expect("granted");
    assert_eq!(committed_at(dir.path()), [pair("steward", "devops")]);
    // Written by you here, so the Notice is a teammate's and not yours.
    assert_eq!(crate::dispatchgrant::changed(dir.path()), None);
}

#[test]
fn a_teammate_s_change_is_told_once_and_your_own_after_it_does_not_hide_it() {
    let dir = project(PROJECT);
    fs::write(
        crate::names::manifest(dir.path()),
        format!("{PROJECT}\n[dispatch.grants]\nqa = [\"devops\"]\n"),
    )
    .expect("a teammate's push");
    let told = crate::dispatchgrant::changed(dir.path()).expect("a change");
    assert_eq!(told.added, ["qa -> devops"]);
    assert_eq!(told.removed, Vec::<String>::new());
    // A grant of your own while that one waits is yours, seen as it is written; the
    // teammate's still waits, in force for no chat here until you allow it.
    grant(dir.path(), &pair("steward", "devops")).expect("granted");
    let both = crate::dispatchgrant::changed(dir.path()).expect("still a change");
    assert_eq!(both.added, ["qa -> devops"]);
    assert_eq!(
        crate::dispatchgrant::InForce::read(dir.path(), Vec::new()).project,
        [pair("steward", "devops")]
    );
    // Read once, it is not told again.
    crate::dispatchgrant::acknowledge(dir.path(), &both.now).expect("kept");
    assert_eq!(crate::dispatchgrant::changed(dir.path()), None);
    // Until it changes again.
    revoke(dir.path(), &pair("qa", "devops")).expect("revoked");
    assert_eq!(crate::dispatchgrant::changed(dir.path()), None, "your own");
    fs::write(crate::names::manifest(dir.path()), PROJECT).expect("a teammate's push");
    assert_eq!(
        crate::dispatchgrant::changed(dir.path())
            .expect("a change")
            .removed,
        ["steward -> devops"]
    );
}

#[test]
fn a_revoke_of_the_project_s_grant_edits_the_committed_file() {
    let dir = project(PROJECT);
    grant(dir.path(), &pair("steward", "devops")).expect("granted");
    revoke(dir.path(), &pair("steward", "devops")).expect("revoked");
    assert_eq!(committed_at(dir.path()), []);
    assert_eq!(
        fs::read_to_string(crate::names::manifest(dir.path())).expect("read"),
        PROJECT
    );
}

#[test]
fn a_project_with_no_committed_file_keeps_no_grant_for_everyone() {
    let dir = tempfile::tempdir().expect("a folder");
    let why = grant(dir.path(), &pair("steward", "devops")).expect_err("refused");
    assert!(
        why.ends_with("is not there, so nothing was changed."),
        "{why}"
    );
}

#[test]
fn a_pair_the_file_already_holds_is_yours_once_you_allow_it_and_the_file_is_not_rewritten() {
    // D-1437-R1: a teammate's pair asks on a chat's tab; Allow for everyone writes nothing new
    // and puts it in force here.
    let text = format!("{PROJECT}\n[dispatch.grants]\nsteward = [\"devops\"]\n");
    let dir = project(&text);
    assert_eq!(
        crate::dispatchgrant::InForce::read(dir.path(), Vec::new()).project,
        []
    );
    grant(dir.path(), &pair("steward", "devops")).expect("granted");
    assert_eq!(
        fs::read_to_string(crate::names::manifest(dir.path())).expect("read"),
        text
    );
    assert_eq!(
        crate::dispatchgrant::InForce::read(dir.path(), Vec::new()).project,
        [pair("steward", "devops")]
    );
}

#[test]
fn whether_a_grant_can_be_written_is_known_before_anything_is_recorded() {
    // The audit comes before the write, so a write that would be refused is found first.
    let dir = project("[dispatch]\ngrants = [\"steward\"]\n");
    let why = can_grant(dir.path(), &pair("steward", "devops")).expect_err("refused");
    assert!(
        why.contains("is not written in a form purlis edits"),
        "{why}"
    );
    let none = tempfile::tempdir().expect("a folder");
    let why = can_grant(none.path(), &pair("steward", "devops")).expect_err("refused");
    assert!(
        why.ends_with("is not there, so nothing was changed."),
        "{why}"
    );
    assert_eq!(
        can_grant(project(PROJECT).path(), &pair("steward", "devops")),
        Ok(())
    );
    // Asking changes nothing.
    let dir = project(PROJECT);
    can_grant(dir.path(), &pair("steward", "devops")).expect("it can");
    assert_eq!(committed_at(dir.path()), []);
}

// ---- a persona renamed (#1504) ------------------------------------------------------------------

#[test]
fn a_rename_is_written_through_the_table_as_asking_persona_and_as_target() {
    let text = "schema = 1 # ours\n\n[dispatch.grants]\ndevops = [\"qa\", \"*\"] # theirs\nsteward = [\"devops\", \"qa\"]\nqa = [\"prod\"]\n\n[sandbox]\nmode = \"on\"\n";
    let after = renamed(text, "devops", "ops").expect("renamed");
    let read = committed(Some(&after));
    let mut pairs = read.pairs.clone();
    pairs.sort();
    assert_eq!(
        pairs,
        [
            pair("ops", "qa"),
            pair("qa", "prod"),
            pair("steward", "ops"),
            pair("steward", "qa")
        ],
        "{after}"
    );
    assert_eq!(read.any, ["ops"]);
    assert_eq!(read.refused, Vec::<String>::new());
    assert!(!after.contains("devops"), "{after}");
    assert!(after.starts_with("schema = 1 # ours\n"), "{after}");
    assert!(
        after.contains("qa = [\"prod\"]\n"),
        "an untouched list is left as written: {after}"
    );
    assert!(after.ends_with("[sandbox]\nmode = \"on\"\n"), "{after}");
}

#[test]
fn a_rename_joins_a_list_the_new_name_has_and_keeps_no_pair_twice_or_to_itself() {
    let text = "[dispatch.grants]\nops = [\"prod\", \"qa\"]\ndevops = [\"qa\", \"ops\", \"steward\"]\nsteward = [\"devops\", \"ops\"]\n";
    let after = renamed(text, "devops", "ops").expect("renamed");
    let read = committed(Some(&after));
    let mut pairs = read.pairs.clone();
    pairs.sort();
    assert_eq!(
        pairs,
        [
            pair("ops", "prod"),
            pair("ops", "qa"),
            pair("ops", "steward"),
            pair("steward", "ops")
        ]
    );
    assert_eq!(read.refused, Vec::<String>::new(), "{after}");
}

#[test]
fn a_rename_leaves_a_file_that_does_not_name_the_persona_byte_for_byte() {
    for text in [
        PROJECT,
        "[dispatch.grants]\nsteward = [\"qa\"]\n",
        "[dispatch]\ndepth = 2\n",
    ] {
        assert_eq!(renamed(text, "devops", "ops").expect("read"), text);
    }
}

#[test]
fn a_rename_refuses_a_table_it_cannot_edit_and_names_that_are_no_persona_s() {
    assert!(renamed("[dispatch.grants]\ndevops = \"qa\"\n", "devops", "ops").is_err());
    assert!(renamed("[dispatch]\ngrants = 3\n", "devops", "ops").is_err());
    assert!(renamed("not toml [", "devops", "ops").is_err());
    for (from, to) in [("devops", "*"), ("*", "ops"), ("devops", "a b")] {
        assert!(renamed(PROJECT, from, to).is_err(), "{from} to {to}");
    }
}

#[test]
fn a_rename_in_a_project_with_no_project_file_is_nothing_to_do() {
    let dir = tempfile::tempdir().expect("a project");
    assert_eq!(rename(dir.path(), "devops", "ops"), Ok(()));
    assert!(!crate::names::manifest(dir.path()).exists());
}
