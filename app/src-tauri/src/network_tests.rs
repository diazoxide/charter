//! Settings' Network page and Blocked lately's Allow (#1662), from a plane's text and a network
//! record under a temporary data home.

use super::*;
use purlis_core::sandboxblock::{Block, Kind, Operation};

const HOST: Block = Block {
    operation: Operation::Connect,
    kind: Kind::Host,
    ours: false,
};

fn plane(text: &str) -> sandbox::Plane {
    sandbox::Plane::of(Some(text))
}

fn chat(name: &str) -> record::Chat {
    record::Chat {
        id: Some(format!("id-{name}")),
        name: Some(name.to_owned()),
        session: Some(2),
    }
}

/// This thread's policy until it is dropped, a panic included.
struct Under;

impl Under {
    fn policy(json: &str) -> Self {
        sandbox::policy::set_for_this_test(sandbox::policy::Locks::parse(
            json,
            std::path::Path::new("/etc/purlis/policy.json"),
        ));
        Self
    }
}

impl Drop for Under {
    fn drop(&mut self) {
        sandbox::policy::set_for_this_test(sandbox::policy::Locks::none());
    }
}

#[test]
fn open_hosts_name_every_host_of_each_preset_in_force_and_then_the_projects_own() {
    let dir = tempfile::tempdir().expect("a project");
    let plane = plane(
        "[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"forge\"]\n\
         hosts = [\"api.example.com\"]\n\n[[forge]]\nkind = \"gitlab\"\nhost = \"git.example.org\"\n",
    );
    let shown = network_of(dir.path(), &plane, None, 100);
    assert!(shown.on);
    let none = sandbox::policy::Locks::none();
    assert_eq!(
        shown.open,
        vec![
            OpenHosts {
                title: "AI providers".to_owned(),
                hosts: sandbox::hosts(&[sandbox::Preset::ModelProviders], &plane, &none),
            },
            OpenHosts {
                title: "Code hosting".to_owned(),
                hosts: sandbox::hosts(&[sandbox::Preset::Forge], &plane, &none),
            },
            OpenHosts {
                title: "This project's hosts".to_owned(),
                hosts: vec!["api.example.com".to_owned()],
            },
        ]
    );
    assert!(shown.open[1].hosts.contains(&"git.example.org".to_owned()));
    assert!(shown.open.iter().all(|one| !one.hosts.is_empty()));
}

#[test]
fn a_project_whose_chats_are_not_sandboxed_lists_nothing() {
    let dir = tempfile::tempdir().expect("a project");
    let shown = network_of(dir.path(), &plane("schema = 1\n"), None, 100);
    assert_eq!(
        shown,
        SandboxNetwork {
            on: false,
            open: Vec::new(),
            blocked: Vec::new(),
        }
    );
}

#[test]
fn blocked_lately_lists_each_host_once_newest_first_with_its_chat_and_allow() {
    let dir = tempfile::tempdir().expect("a home");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).expect("a project");
    let record = Record::in_data(&dir.path().join("data"));
    let write = |target: &str, name: &str, at: u64| {
        record
            .write(
                &root,
                &Entry::blocked(&HOST, Some(target), chat(name), None, at),
            )
            .expect("written");
    };
    write("a.example:443", "first", 10);
    write("api.example.com:443", "first", 20);
    write("a.example:443", "second", 30);
    write("b.example:5432", "second", 40);
    // A file block is no row: the page lists the network.
    record
        .write(
            &root,
            &Entry::blocked(
                &Block {
                    operation: Operation::Write,
                    kind: Kind::Home,
                    ours: false,
                },
                None,
                chat("first"),
                None,
                50,
            ),
        )
        .expect("written");
    let plane = plane("[sandbox]\nmode = \"on\"\negress = []\nhosts = [\"api.example.com\"]\n");
    let shown = network_of(&root, &plane, Some(&record), 60);
    /// A row as the test reads it: target, chat, when, how often, and whether it is reached.
    type Read<'a> = (Option<&'a str>, Option<&'a str>, u32, u32, bool);
    let rows: Vec<Read<'_>> = shown
        .blocked
        .iter()
        .map(|row| {
            (
                row.target.as_deref(),
                row.chat.as_deref(),
                row.at,
                row.times,
                row.reached,
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (Some("b.example:5432"), Some("second"), 40, 1, false),
            (Some("a.example:443"), Some("second"), 30, 2, false),
            (Some("api.example.com:443"), Some("first"), 20, 1, true),
        ]
    );
    assert_eq!(
        shown.blocked[0].levels,
        vec![GrantLevel::You, GrantLevel::Project]
    );
    // Reached already: nothing to allow.
    assert_eq!(shown.blocked[2].levels, Vec::new());
    assert_eq!(
        shown.blocked[0].said,
        "a connection to an internet host this project does not allow"
    );
}

#[test]
fn blocked_lately_offers_no_allow_policy_forbids() {
    let dir = tempfile::tempdir().expect("a home");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).expect("a project");
    let record = Record::in_data(&dir.path().join("data"));
    record
        .write(
            &root,
            &Entry::blocked(&HOST, Some("a.example:443"), chat("one"), None, 10),
        )
        .expect("written");
    let _under = Under::policy(r#"{"owner": "IT", "sandbox": {"hosts": ["*.corp.example"]}}"#);
    let shown = network_of(
        &root,
        &plane("[sandbox]\nmode = \"on\"\negress = []\n"),
        Some(&record),
        20,
    );
    assert_eq!(shown.blocked.len(), 1);
    assert_eq!(shown.blocked[0].levels, Vec::new());
}

fn audited_into(
    seen: &std::sync::Mutex<Vec<(bool, String, String)>>,
) -> impl Fn(Option<u32>, &grant::Audited<'_>) -> Result<(), String> + '_ {
    move |_, audited| {
        seen.lock().expect("the audit").push((
            audited.granted,
            audited.target.to_owned(),
            audited.level.word().to_owned(),
        ));
        Ok(())
    }
}

#[test]
fn allow_on_blocked_lately_is_judged_audited_and_never_for_one_chat() {
    let dir = tempfile::tempdir().expect("a project");
    let chats = crate::chats::Chats::new();
    let seen = std::sync::Mutex::new(Vec::new());
    let audit = audited_into(&seen);
    let refused = allow_blocked(
        dir.path(),
        &chats,
        "a.example:443",
        GrantLevel::Chat,
        &audit,
        1,
    )
    .expect_err("chat");
    assert!(refused.contains("that chat's own Notice"), "{refused}");
    let refused = allow_blocked(
        dir.path(),
        &chats,
        "169.254.169.254",
        GrantLevel::You,
        &audit,
        1,
    )
    .expect_err("never grantable");
    assert!(refused.contains("link-local"), "{refused}");
    let unaudited = |_: Option<u32>, _: &grant::Audited<'_>| Err("no event log".to_owned());
    assert!(
        allow_blocked(
            dir.path(),
            &chats,
            "a.example:443",
            GrantLevel::You,
            &unaudited,
            1
        )
        .is_err()
    );
    assert!(sandbox::hosts::personal(dir.path()).is_empty());
    assert!(seen.lock().expect("the audit").is_empty());
}

/// Writes the project's local settings file, which a sandboxed run of this suite cannot: it
/// first runs on CI.
#[test]
fn allow_on_blocked_lately_keeps_the_host_for_this_project_on_this_machine_and_records_it() {
    let dir = tempfile::tempdir().expect("a home");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).expect("a project");
    let record = Record::in_data(&dir.path().join("data"));
    let chats = crate::chats::Chats::new();
    let seen = std::sync::Mutex::new(Vec::new());
    let audit = audited_into(&seen);
    let audit = recorded(&audit, Some(&record), &root, &chats);
    let said =
        allow_blocked(&root, &chats, "a.example:443", GrantLevel::You, &audit, 5).expect("allowed");
    assert_eq!(
        said.said,
        "Allowed a.example:443 for me on this machine. A chat that is running reaches it from \
         its next start."
    );
    assert_eq!(
        *seen.lock().expect("the audit"),
        vec![(true, "a.example:443".to_owned(), "you".to_owned())]
    );
    assert_eq!(
        sandbox::hosts::personal(&root)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["a.example:443".to_owned()]
    );
    let kept = record.read(&root, now_secs());
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].event, record::Event::Allow);
    assert_eq!(kept[0].target.as_deref(), Some("a.example:443"));
    assert_eq!(kept[0].scope.as_deref(), Some("you"));
    assert_eq!(kept[0].who.as_deref(), Some("you"));
}

#[test]
fn a_removal_is_recorded_after_its_audit_and_not_without_one() {
    let dir = tempfile::tempdir().expect("a home");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).expect("a project");
    let record = Record::in_data(&dir.path().join("data"));
    let chats = crate::chats::Chats::new();
    let refusing = |_: Option<u32>, _: &grant::Audited<'_>| Err("no event log".to_owned());
    let removed = grant::Audited {
        granted: false,
        what: "host",
        target: "a.example:443",
        level: grant::Level::Project,
    };
    assert!(recorded(&refusing, Some(&record), &root, &chats)(None, &removed).is_err());
    assert!(
        record.read(&root, now_secs()).is_empty(),
        "no audit, no record"
    );
    let passing = |_: Option<u32>, _: &grant::Audited<'_>| Ok(());
    recorded(&passing, Some(&record), &root, &chats)(None, &removed).expect("recorded");
    let kept = record.read(&root, now_secs());
    assert_eq!(kept[0].event, record::Event::Remove);
    assert_eq!(kept[0].scope.as_deref(), Some("project"));
}

#[test]
fn a_chat_that_is_not_open_has_nothing_to_say() {
    let dir = tempfile::tempdir().expect("a project");
    let shown = chat_network_of(dir.path(), &crate::chats::Chats::new(), 4, None, 100);
    assert_eq!(
        shown,
        ChatNetwork {
            open: false,
            sandboxed: false,
            reach: Vec::new(),
            refused: Vec::new(),
        }
    );
}

#[test]
fn a_chat_reaches_each_host_by_why_open_persona_or_allowed() {
    let reached = reach_of(
        &[
            "api.anthropic.com".to_owned(),
            "db.internal:5432".to_owned(),
            "extra.example:443".to_owned(),
        ],
        &["api.anthropic.com".to_owned()],
        &["db.internal:5432".to_owned()],
    );
    assert_eq!(
        reached.iter().map(|one| one.by).collect::<Vec<_>>(),
        vec![ReachedBy::Open, ReachedBy::Persona, ReachedBy::Allowed]
    );
}

#[test]
fn a_chat_is_shown_only_its_own_refusals_by_its_id_across_its_numbers() {
    let mine = record::Chat {
        id: Some("01MINE".to_owned()),
        name: Some("mine".to_owned()),
        session: Some(7),
    };
    let other_with_my_number = record::Chat {
        id: Some("01OTHER".to_owned()),
        name: Some("other".to_owned()),
        session: Some(3),
    };
    let entries = vec![
        Entry::blocked(&HOST, Some("a.example:443"), mine.clone(), None, 10),
        Entry::blocked(&HOST, Some("b.example:443"), other_with_my_number, None, 20),
        Entry::blocked(
            &HOST,
            Some("c.example:443"),
            record::Chat {
                session: Some(3),
                ..mine
            },
            None,
            30,
        ),
    ];
    let rows = refused_of(
        &entries,
        (Some("01MINE"), 3),
        &["c.example".to_owned()],
        &sandbox::policy::Locks::none(),
    );
    assert_eq!(
        rows.iter()
            .map(|row| (row.target.as_deref(), row.reached, row.chat.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            (Some("c.example:443"), true, None),
            (Some("a.example:443"), false, None),
        ]
    );
}
