//! Adding and removing a sandbox's hosts in Settings, at both levels (#1341).

use std::fs;
use std::path::Path;

use super::*;

/// A project whose local file git ignores, as `purlis init` makes one.
fn project(shared: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), shared).unwrap();
    crate::testgit::run(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join(".gitignore"), "/charter.local.toml\n").unwrap();
    dir
}

fn text(root: &Path, file: &str) -> Option<String> {
    fs::read_to_string(root.join(file)).ok()
}

const ON: &str = "schema = 1\n\n[sandbox]\nmode = \"on\" # on for everyone\n";

#[test]
fn a_project_host_is_added_to_the_committed_file_keeping_every_other_line() {
    let dir = project(ON);
    let id = add(dir.path(), Which::Shared, Some(ON), "10.100.39.145:6443").expect("added");
    let after = text(dir.path(), "charter.toml").unwrap();
    assert_eq!(
        after,
        "schema = 1\n\n[sandbox]\nmode = \"on\" # on for everyone\nhosts = [\"10.100.39.145:6443\"]\n"
    );
    assert_eq!(listed(&after)[0].id, id);
    assert_eq!(listed(&after)[0].label, "10.100.39.145:6443");
    // Written by you here, so the Notice is a teammate's and not yours.
    assert_eq!(crate::sandbox::local::hosts_changed(dir.path()), None);
}

#[test]
fn a_host_that_is_not_one_is_refused_under_its_field_and_nothing_is_written() {
    let dir = project(ON);
    let refused = add(
        dir.path(),
        Which::Shared,
        Some(ON),
        "https://api.example.com/v1",
    )
    .expect_err("refused");
    assert_eq!(refused.fields.len(), 1);
    assert_eq!(refused.fields[0].field, "host");
    assert!(refused.fields[0].why.contains("URL"), "{refused:?}");
    assert_eq!(text(dir.path(), "charter.toml").unwrap(), ON);
}

#[test]
fn a_host_already_listed_is_refused() {
    let with = format!("{ON}hosts = [\"a.example\"]\n");
    let dir = project(&with);
    let refused = add(dir.path(), Which::Shared, Some(&with), "A.example.").expect_err("refused");
    assert_eq!(refused.fields[0].why, "a.example is already listed here.");
}

#[test]
fn your_own_host_goes_to_this_machine_s_file_alone() {
    let dir = project(ON);
    add(dir.path(), Which::Local, None, "[fd00::7]:8443").expect("added");
    assert_eq!(text(dir.path(), "charter.toml").unwrap(), ON);
    assert_eq!(
        text(dir.path(), "charter.local.toml").unwrap(),
        "[sandbox]\nhosts = [\"[fd00::7]:8443\"]\n"
    );
    assert_eq!(
        crate::sandbox::hosts::personal(dir.path()),
        [crate::sandbox::hosts::Host::parse("[fd00::7]:8443").unwrap()]
    );
}

/// Review of #1341, round 3: a host a chat wrote into this machine's file grants nothing and
/// is listed "not yet confirmed"; Confirm makes it yours, and Settings' own Add is confirmed.
#[test]
fn a_host_you_did_not_add_here_waits_for_your_confirm() {
    let dir = project(ON);
    let chat = "[sandbox]\nhosts = [\"10.0.0.6\"]\n";
    fs::write(dir.path().join("charter.local.toml"), chat).unwrap();
    assert_eq!(
        crate::sandbox::hosts::personal(dir.path()),
        Vec::<crate::sandbox::hosts::Host>::new()
    );
    let shown = listed_at(dir.path(), Which::Local, chat);
    assert_eq!(shown[0].label, "10.0.0.6 (not yet confirmed)");
    assert!(shown[0].values.contains(&("confirmed", "no".to_owned())));
    let refused = add(dir.path(), Which::Local, Some(chat), "10.0.0.6").expect_err("listed");
    assert!(
        refused.fields[0].why.contains("press Confirm"),
        "{refused:?}"
    );

    confirm(dir.path(), Some(chat), &shown[0].id).expect("confirmed");

    assert_eq!(
        crate::sandbox::hosts::personal(dir.path()),
        [crate::sandbox::hosts::Host::parse("10.0.0.6").unwrap()]
    );
    assert_eq!(
        listed_at(dir.path(), Which::Local, chat)[0].label,
        "10.0.0.6"
    );

    let base = text(dir.path(), "charter.local.toml");
    remove(dir.path(), Which::Local, base.as_deref(), &shown[0].id).expect("removed");
    assert_eq!(
        crate::sandbox::local::confirmed_hosts(dir.path()),
        Vec::<String>::new()
    );
}

#[test]
fn your_own_host_that_the_project_already_has_is_refused() {
    let with = format!("{ON}hosts = [\"a.example\"]\n");
    let dir = project(&with);
    let refused = add(dir.path(), Which::Local, None, "a.example").expect_err("refused");
    assert!(
        refused.fields[0]
            .why
            .contains("already one of the project's hosts"),
        "{refused:?}"
    );
}

#[test]
fn removing_the_last_host_takes_the_key_and_this_machine_s_empty_table_with_it() {
    let dir = project(ON);
    add(dir.path(), Which::Local, None, "b.example").expect("added");
    let base = text(dir.path(), "charter.local.toml").unwrap();
    let id = listed(&base)[0].id.clone();
    let took = remove(dir.path(), Which::Local, Some(&base), &id).expect("removed");
    assert_eq!(took, "b.example");
    assert_eq!(text(dir.path(), "charter.local.toml").unwrap(), "");

    let with = format!("{ON}hosts = [\"a.example\", \"c.example\"]\n");
    fs::write(dir.path().join("charter.toml"), &with).unwrap();
    let id = listed(&with)[0].id.clone();
    remove(dir.path(), Which::Shared, Some(&with), &id).expect("removed");
    assert_eq!(
        text(dir.path(), "charter.toml").unwrap(),
        format!("{ON}hosts = [\"c.example\"]\n")
    );
}

#[test]
fn an_entry_that_reaches_nothing_is_listed_with_why_so_it_can_be_removed() {
    let with = format!("{ON}hosts = [\"127.0.0.1\"]\n");
    let listed = listed(&with);
    assert_eq!(listed.len(), 1);
    assert!(
        listed[0]
            .label
            .starts_with("127.0.0.1 (reaches nothing: 127.0.0.1 is this machine."),
        "{}",
        listed[0].label
    );
}

#[test]
fn a_remove_sent_for_what_is_no_longer_there_is_refused() {
    let with = format!("{ON}hosts = [\"a.example\"]\n");
    let dir = project(&with);
    let refused = remove(dir.path(), Which::Shared, Some(&with), "host:0:0000").expect_err("gone");
    assert!(refused.file[0].contains("not in charter.toml as it was shown"));
}

// What a write makes of the text, without a file (these run where a manifest cannot be written).

fn host(typed: &str) -> crate::sandbox::hosts::Host {
    crate::sandbox::hosts::Host::parse(typed).unwrap()
}

#[test]
fn a_host_is_written_last_in_one_list_keeping_every_other_line() {
    assert_eq!(
        with(Which::Shared, ON, &host("10.100.39.145:6443")).unwrap(),
        format!("{ON}hosts = [\"10.100.39.145:6443\"]\n")
    );
    let across = format!("{ON}hosts = [\n  \"a.example\",\n]\n");
    assert_eq!(
        with(Which::Shared, &across, &host("b.example")).unwrap(),
        format!("{ON}hosts = [\"a.example\", \"b.example\"]\n")
    );
    assert_eq!(
        with(Which::Local, "", &host("[fd00::7]:8443")).unwrap(),
        "[sandbox]\nhosts = [\"[fd00::7]:8443\"]\n"
    );
}

#[test]
fn removing_writes_the_rest_as_one_list_and_the_last_takes_its_key_with_it() {
    let two = format!("{ON}hosts = [\n  \"a.example\",\n  \"c.example\",\n]\n");
    assert_eq!(
        without(Which::Shared, &two, 0).unwrap(),
        format!("{ON}hosts = [\"c.example\"]\n")
    );
    let one = format!("{ON}hosts = [\"a.example\"]\n");
    assert_eq!(without(Which::Shared, &one, 0).unwrap(), ON);
    assert_eq!(
        without(Which::Local, "[sandbox]\nhosts = [\"b.example\"]\n", 0).unwrap(),
        ""
    );
}

#[test]
fn a_sandbox_that_is_not_a_table_or_hosts_that_are_not_a_list_are_left_to_edit_as_toml() {
    assert!(with(Which::Shared, "sandbox = 1\n", &host("a.example")).is_err());
    assert!(with(Which::Shared, "[sandbox]\nhosts = 1\n", &host("a.example")).is_err());
}

#[test]
fn a_notice_grants_and_settings_revokes_a_host_against_the_file_as_it_is_now() {
    // #1342 and #1348: the same rules Settings' Add and Remove keep, read from disk.
    let dir = project(ON);
    let host = Host::parse("api.example.com").unwrap();
    grant(dir.path(), Which::Local, &host).expect("yours");
    assert_eq!(
        crate::sandbox::hosts::personal(dir.path()),
        std::slice::from_ref(&host),
        "granted from a Notice, so confirmed on this machine"
    );
    grant(
        dir.path(),
        Which::Shared,
        &Host::parse("10.0.0.5:6443").unwrap(),
    )
    .expect("the project's");
    assert!(
        text(dir.path(), "charter.toml")
            .unwrap()
            .contains("10.0.0.5:6443")
    );
    // Granted twice is said, not written twice.
    assert!(grant(dir.path(), Which::Local, &host).is_err());

    revoke(dir.path(), Which::Local, &host).expect("revoked");
    assert!(crate::sandbox::hosts::personal(dir.path()).is_empty());
    revoke(
        dir.path(),
        Which::Shared,
        &Host::parse("10.0.0.5:6443").unwrap(),
    )
    .expect("revoked");
    assert!(
        !text(dir.path(), "charter.toml")
            .unwrap()
            .contains("10.0.0.5")
    );
    // Revoking what is not there changes nothing.
    revoke(dir.path(), Which::Shared, &host).expect("nothing to do");
}

// ---- an administrator's policy (#1423) ----

#[test]
fn a_host_policy_does_not_allow_is_refused_as_it_is_added_and_nothing_is_written() {
    use crate::sandbox::policy::{Locks, set_for_this_test};
    let dir = project(ON);
    set_for_this_test(Locks::parse(
        r#"{"owner": "IT", "sandbox": {"hosts": ["*.corp.example"], "personal-hosts": false}}"#,
        Path::new("/etc/purlis/policy.json"),
    ));
    let project_s = add(dir.path(), Which::Shared, Some(ON), "pastebin.example");
    let allowed = add(dir.path(), Which::Shared, Some(ON), "build.corp.example");
    let yours = add(dir.path(), Which::Local, None, "git.corp.example");
    set_for_this_test(Locks::none());

    let refused = project_s.expect_err("refused");
    assert_eq!(refused.fields.len(), 1);
    assert_eq!(refused.fields[0].field, "host");
    assert_eq!(
        refused.fields[0].why,
        "pastebin.example is not a host policy allows. Locked by policy, set by IT in \
         /etc/purlis/policy.json."
    );
    // One the policy allows is written; one of yours is refused where policy forbids yours.
    allowed.expect("added");
    assert_eq!(
        text(dir.path(), "charter.toml").unwrap(),
        format!("{ON}hosts = [\"build.corp.example\"]\n")
    );
    let refused = yours.expect_err("refused");
    assert!(
        refused.fields[0].why.starts_with(
            "git.corp.example is a host of yours, and policy forbids hosts of your own."
        ),
        "{refused:?}"
    );
    assert_eq!(text(dir.path(), "charter.local.toml"), None);
}
