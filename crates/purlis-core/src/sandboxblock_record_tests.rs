//! The network record (#1662): what is written for a block and an Allow, what is read back, and
//! what is let go of. Driven through [`Record`] alone.

use super::*;
use crate::sandboxblock::{Block, Kind, Operation};

const DAY: u64 = 24 * 60 * 60;

fn project() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).expect("a project");
    (dir, root)
}

fn chat(id: &str, name: &str) -> Chat {
    Chat {
        id: Some(id.to_owned()),
        name: Some(name.to_owned()),
        session: Some(3),
    }
}

const HOST_BLOCK: Block = Block {
    operation: Operation::Connect,
    kind: Kind::Host,
    ours: false,
};

#[test]
fn a_host_block_is_read_back_with_its_chat_persona_and_host_and_kept_outside_the_project() {
    let (dir, root) = project();
    let record = Record::in_data(&dir.path().join("data"));
    let entry = Entry::blocked(
        &HOST_BLOCK,
        Some("api.example.com:443"),
        chat("01J", "fix the build"),
        Some("steward"),
        1_000,
    );
    record.write(&root, &entry).expect("written");

    let read = record.read(&root, 1_000);
    assert_eq!(read, vec![entry]);
    assert_eq!(read[0].target.as_deref(), Some("api.example.com:443"));
    assert_eq!(read[0].persona.as_deref(), Some("steward"));
    assert_eq!(read[0].outcome, Outcome::Refused);
    assert!(record.file(&root).starts_with(dir.path().join("data")));
    assert!(
        !record.file(&root).starts_with(&root),
        "never in the project"
    );
}

#[test]
fn only_a_host_a_connection_was_refused_is_kept_and_only_as_a_host() {
    let write = Block {
        operation: Operation::Write,
        kind: Kind::Home,
        ours: false,
    };
    let path = Entry::blocked(
        &write,
        Some("/Users/dev/secret/notes"),
        Chat::default(),
        None,
        1,
    );
    assert_eq!(path.target, None, "a path is never kept");
    let junk = Entry::blocked(
        &HOST_BLOCK,
        Some("not a host; rm -rf"),
        Chat::default(),
        None,
        1,
    );
    assert_eq!(junk.target, None, "what is not a host is never kept");
    let url = Entry::blocked(
        &HOST_BLOCK,
        Some("api.example.com:8443"),
        Chat::default(),
        None,
        1,
    );
    assert_eq!(url.target.as_deref(), Some("api.example.com:8443"));
}

#[test]
fn each_project_keeps_its_own_record() {
    let (dir, root) = project();
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).expect("another project");
    let record = Record::in_data(&dir.path().join("data"));
    record
        .write(
            &root,
            &Entry::blocked(&HOST_BLOCK, Some("a.example"), Chat::default(), None, 5),
        )
        .expect("written");
    assert_eq!(record.read(&other, 5), Vec::new());
    assert_eq!(record.read(&root, 5).len(), 1);
}

#[test]
fn what_is_older_than_thirty_days_is_neither_read_nor_kept() {
    let (dir, root) = project();
    let record = Record::in_data(&dir.path().join("data"));
    let now = 100 * DAY;
    let old = Entry::blocked(
        &HOST_BLOCK,
        Some("old.example"),
        Chat::default(),
        None,
        now - 31 * DAY,
    );
    let kept = Entry::blocked(
        &HOST_BLOCK,
        Some("kept.example"),
        Chat::default(),
        None,
        now - 29 * DAY,
    );
    record.write(&root, &old).expect("written");
    record.write(&root, &kept).expect("written");
    assert_eq!(record.read(&root, now), vec![kept.clone()]);

    let fresh = Entry::allowed("host", "kept.example", "you", chat("01J", "fix"), None, now);
    record.write(&root, &fresh).expect("written");
    let text = std::fs::read_to_string(record.file(&root)).expect("the file");
    assert!(
        !text.contains("old.example"),
        "let go of at the next write: {text}"
    );
    assert_eq!(text.lines().count(), 2, "{text}");
}

#[test]
fn an_allow_and_its_removal_say_what_where_who_and_the_outcome() {
    let (dir, root) = project();
    let record = Record::in_data(&dir.path().join("data"));
    let allowed = Entry::allowed(
        "host",
        "db.example:5432",
        "you",
        chat("01J", "fix"),
        None,
        10,
    );
    let removed = Entry::removed("host", "db.example:5432", "you", 20);
    record.write(&root, &allowed).expect("written");
    record.write(&root, &removed).expect("written");
    let read = record.read(&root, 20);
    assert_eq!(read[0].event, Event::Allow);
    assert_eq!(read[0].outcome, Outcome::Allowed);
    assert_eq!(read[0].scope.as_deref(), Some("you"));
    assert_eq!(read[0].who.as_deref(), Some("you"));
    assert_eq!(read[1].event, Event::Remove);
    assert_eq!(read[1].outcome, Outcome::Removed);
}

#[test]
fn a_line_this_build_cannot_read_is_kept_and_passed_over() {
    let (dir, root) = project();
    let record = Record::in_data(&dir.path().join("data"));
    let file = record.file(&root);
    std::fs::create_dir_all(file.parent().expect("a folder")).expect("the folder");
    std::fs::write(
        &file,
        "{\"at\":9,\"event\":\"a-newer-word\"}\nnot json at all\n",
    )
    .expect("planted");
    let one = Entry::blocked(&HOST_BLOCK, Some("a.example"), Chat::default(), None, 10);
    record.write(&root, &one).expect("written");
    assert_eq!(record.read(&root, 10), vec![one]);
    let text = std::fs::read_to_string(&file).expect("the file");
    assert!(text.contains("a-newer-word"), "{text}");
}

#[test]
fn the_doctor_counts_seven_days_of_blocks_per_operation() {
    let now = 100 * DAY;
    let ours = Block {
        operation: Operation::Write,
        kind: Kind::ProjectFiles,
        ours: true,
    };
    let entries = vec![
        Entry::blocked(&ours, None, Chat::default(), None, now - 2 * DAY),
        Entry::blocked(&ours, None, Chat::default(), None, now - 9 * DAY),
        Entry::blocked(
            &HOST_BLOCK,
            Some("a.example:443"),
            Chat::default(),
            None,
            now - DAY,
        ),
        Entry::blocked(
            &HOST_BLOCK,
            Some("a.example:443"),
            Chat::default(),
            None,
            now - 60,
        ),
        Entry::blocked(
            &HOST_BLOCK,
            Some("b.example:443"),
            Chat::default(),
            None,
            now - 60,
        ),
        Entry::allowed("host", "b.example:443", "you", Chat::default(), None, now),
    ];
    assert_eq!(
        counts(&entries, now),
        vec![
            Count {
                operation: Operation::Write,
                blocks: 1,
                ours: 1
            },
            Count {
                operation: Operation::Connect,
                blocks: 3,
                ours: 0
            },
        ]
    );
    assert_eq!(
        hosts_refused(&entries, now),
        vec![
            ("a.example:443".to_owned(), 2),
            ("b.example:443".to_owned(), 1)
        ]
    );
}

#[test]
fn the_file_is_this_users_alone() {
    let (dir, root) = project();
    let record = Record::in_data(&dir.path().join("data"));
    record
        .write(
            &root,
            &Entry::blocked(&HOST_BLOCK, None, Chat::default(), None, 1),
        )
        .expect("written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(record.file(&root))
            .expect("the file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }
}

#[test]
fn what_is_kept_of_a_hooks_blocks_names_no_path_argument_or_output() {
    let (dir, root) = project();
    let record = Record::in_data(&dir.path().join("data"));
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": "purlis session record --title secret-title"},
        "error": "Exit code 1\nprinted output\n<sandbox_violations>\npurlis(48211) deny(1) \
                  file-write-create /Users/dev/proj/workspaces/alpha/sessions/secret-title.md\n\
                  deny network-outbound api.example.com:443 (host is not on the allow list)\n\
                  </sandbox_violations>",
    });
    let cwd = root.join("workspaces/alpha");
    let place = crate::sandboxblock::Place {
        root: &root,
        chat: &cwd,
        cwd: &cwd,
        home: Some(std::path::Path::new("/Users/dev")),
    };
    for (block, target) in crate::sandboxblock::detect_with_targets(&payload, &place) {
        let entry = Entry::blocked(&block, target.as_deref(), Chat::default(), None, 1);
        record.write(&root, &entry).expect("written");
    }
    let text = std::fs::read_to_string(record.file(&root)).expect("the file");
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.contains("api.example.com:443"), "{text}");
    for leak in [
        "/Users",
        "sessions",
        "secret-title",
        "48211",
        "alpha",
        "printed",
    ] {
        assert!(!text.contains(leak), "{leak} in {text}");
    }
}

/// #1664: connections purlis's own proxy carried are a `connect` line, with the host and port,
/// the layer that let them through and how many; never a Block, and never anything but a host.
#[test]
fn connections_the_proxy_carried_are_kept_with_their_layer_and_count() {
    let (dir, root) = project();
    let record = Record::in_data(&dir.path().join("data"));
    let carried = Entry::connected(
        Some("registry.npmjs.org:443"),
        "open",
        42,
        chat("01J", "install"),
        Some("steward"),
        1_000,
    );
    let past_the_tally = Entry::connected(None, "you", 3, chat("01J", "install"), None, 1_001);
    let not_a_host = Entry::connected(Some("/etc/passwd"), "open", 1, Chat::default(), None, 1_002);
    let refused = Entry::connected(
        Some("example.org:443"),
        "ask",
        2,
        Chat::default(),
        None,
        1_003,
    );
    assert_eq!(refused.outcome, Outcome::Refused);
    assert_eq!(carried.outcome, Outcome::Allowed);
    for entry in [&carried, &past_the_tally, &not_a_host] {
        record.write(&root, entry).expect("written");
    }
    let read = record.read(&root, 2_000);
    assert_eq!(read.len(), 3);
    assert_eq!(read[0], carried);
    assert_eq!(read[0].event, Event::Connect);
    assert_eq!(read[0].scope.as_deref(), Some("open"));
    assert_eq!(read[0].times, Some(42));
    assert_eq!(read[1].target, None);
    assert_eq!(read[2].target, None, "only a host is kept");
    // Not a Block: neither listed as refused nor counted.
    assert!(read.iter().all(|entry| !entry.is_host_block()));
    assert!(counts(&read, 2_000).is_empty());
    assert!(
        std::fs::read_to_string(record.file(&root))
            .expect("the file")
            .contains("\"event\":\"connect\"")
    );
}
