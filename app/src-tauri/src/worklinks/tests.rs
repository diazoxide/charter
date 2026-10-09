use std::path::{Path, PathBuf};

use purlis_core::engine::Size;
use purlis_core::reopen::Chat;
use purlis_core::work::log::{Cause, append_alias, dir_for, fold};

use super::*;
use crate::host::pretend::Pretend;

const DEVICE: &str = "01K6H0Z8Y3V1N3G4QK0A9T5B7C";
const SIZE: Size = Size {
    columns: 80,
    rows: 24,
};
const TODO: &str = "todo:alpha/20261002-080000-port-the-picker";
const ISSUE: &str = "github:github.com/acme/api#12";

fn at(second: u32) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2026, 10, 2, 8, 0, second)
        .unwrap()
}

/// A project with workspaces `alpha` and `beta`, and the todo [`TODO`] names open in `alpha`.
fn project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    for ws in ["alpha", "beta"] {
        std::fs::create_dir_all(root.join("workspaces").join(ws)).unwrap();
    }
    let written = purlis_core::workspaces::Plane::open(&root)
        .workspace("alpha")
        .unwrap()
        .add_todo("Port the picker", at(0).naive_utc())
        .unwrap();
    assert_eq!(
        TODO.strip_prefix("todo:alpha/"),
        written.file_stem().and_then(|s| s.to_str()),
        "the todo `TODO` names is the one written"
    );
    (dir, root)
}

/// Chats on device `device`, on a host that runs nothing.
fn chats_on(device: Option<&str>) -> Chats {
    let mut chats = Chats::on_host(
        Box::new(|_| {}),
        Box::new(Pretend::default()),
        crate::chats::tests::no_project(),
    );
    chats.on_device(device.map(str::to_owned));
    chats
}

/// A chat started in `cwd`, and its id.
fn started_in(chats: &Chats, cwd: &Path) -> (u32, String) {
    let session = chats
        .start(
            &Chat {
                program: "/nowhere/a-program-nothing-runs".to_owned(),
                cwd: Some(cwd.to_path_buf()),
                name: "ide.7".to_owned(),
                ..Default::default()
            },
            SIZE,
        )
        .expect("the host opens it");
    let id = chats.record().chats[0]
        .identity
        .id
        .clone()
        .expect("a chat is given an id");
    (session, id)
}

fn lines(root: &Path, ws: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir_for(root, ws).join(format!("{DEVICE}.jsonl")))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn a_chat_in_a_workspace_is_linked_by_its_id_in_this_devices_log_of_that_workspace() {
    let (_dir, root) = project();
    let chats = chats_on(Some(DEVICE));
    let (session, id) = started_in(&chats, &root.join("workspaces/alpha"));

    assert_eq!(
        link(&root, &chats, session, ISSUE, at(1)),
        Ok(ISSUE.to_owned())
    );

    let written = lines(&root, "alpha");
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["op"], "link");
    assert_eq!(written[0]["item"], ISSUE);
    assert_eq!(written[0]["chat"], id.as_str());
    assert_eq!(item_of(&root, &chats, session), Ok(Some(ISSUE.to_owned())));
}

#[test]
fn a_chat_filed_at_the_project_root_is_refused_and_nothing_is_written() {
    let (_dir, root) = project();
    let chats = chats_on(Some(DEVICE));
    let (session, _) = started_in(&chats, &root);

    let refused = link(&root, &chats, session, ISSUE, at(1)).unwrap_err();
    assert!(refused.contains("in no workspace"), "{refused}");
    let refused = unlink(&root, &chats, session, at(2)).unwrap_err();
    assert!(refused.contains("in no workspace"), "{refused}");
    assert!(refused.contains("unlinked"), "{refused}");
    for ws in ["alpha", "beta"] {
        assert!(!dir_for(&root, ws).exists());
    }
}

#[test]
fn unlinking_ends_the_link_and_answers_the_item_it_named() {
    let (_dir, root) = project();
    let chats = chats_on(Some(DEVICE));
    let (session, _) = started_in(&chats, &root.join("workspaces/alpha"));
    link(&root, &chats, session, ISSUE, at(1)).unwrap();

    assert_eq!(
        unlink(&root, &chats, session, at(2)),
        Ok(Some(ISSUE.to_owned()))
    );
    assert_eq!(item_of(&root, &chats, session), Ok(None));
    assert_eq!(unlink(&root, &chats, session, at(3)), Ok(None));
    assert_eq!(lines(&root, "alpha").len(), 2);
}

#[test]
fn a_chat_linked_to_a_todo_that_is_promoted_works_on_the_issue() {
    let (_dir, root) = project();
    let chats = chats_on(Some(DEVICE));
    let (session, id) = started_in(&chats, &root.join("workspaces/alpha"));
    link(&root, &chats, session, TODO, at(1)).unwrap();

    let todo = TrackerKey::parse(TODO).unwrap();
    let issue = TrackerKey::parse(ISSUE).unwrap();
    append_alias(
        &root,
        "alpha",
        DEVICE,
        at(2),
        todo,
        issue.clone(),
        Cause::Promoted,
    )
    .unwrap();

    assert_eq!(item_of(&root, &chats, session), Ok(Some(ISSUE.to_owned())));
    assert_eq!(fold(&root).chats_on(&issue), vec![id]);
}

#[test]
fn a_todo_key_that_names_no_todo_is_refused_with_the_cores_sentence() {
    let (_dir, root) = project();
    let chats = chats_on(Some(DEVICE));
    let (session, _) = started_in(&chats, &root.join("workspaces/alpha"));

    let refused = link(
        &root,
        &chats,
        session,
        "todo:alpha/20261002-080000-port-the-pickr",
        at(1),
    )
    .unwrap_err();
    assert!(refused.contains("names no todo"), "{refused}");
    assert_eq!(item_of(&root, &chats, session), Ok(None));
    assert!(!dir_for(&root, "alpha").exists(), "nothing was written");
}

#[test]
fn an_item_that_is_not_a_tracker_key_in_its_normal_form_is_refused() {
    let (_dir, root) = project();
    let chats = chats_on(Some(DEVICE));
    let (session, _) = started_in(&chats, &root.join("workspaces/alpha"));

    for not_a_key in [
        "",
        "https://github.com/acme/api/issues/12",
        "github:GitHub.com/acme/api#12",
    ] {
        assert!(
            link(&root, &chats, session, not_a_key, at(1)).is_err(),
            "{not_a_key:?} was taken"
        );
    }
    assert!(!dir_for(&root, "alpha").exists());
}

#[test]
fn a_machine_with_no_device_id_writes_no_work_link() {
    let (_dir, root) = project();
    let chats = chats_on(None);
    let (session, _) = started_in(&chats, &root.join("workspaces/alpha"));

    let refused = link(&root, &chats, session, ISSUE, at(1)).unwrap_err();
    assert!(refused.contains("device id"), "{refused}");
    assert!(!dir_for(&root, "alpha").exists());
}

#[test]
fn a_session_charter_does_not_have_open_is_refused() {
    let (_dir, root) = project();
    let chats = chats_on(Some(DEVICE));

    let refused = link(&root, &chats, 42, ISSUE, at(1)).unwrap_err();
    assert!(refused.contains("no chat 42"), "{refused}");
}

#[test]
fn a_chat_working_outside_the_project_is_told_so_and_not_that_it_is_at_the_project_root() {
    let (_dir, root) = project();
    let elsewhere = tempfile::tempdir().unwrap();
    let chats = chats_on(Some(DEVICE));
    let (session, _) = started_in(&chats, elsewhere.path());

    let refused = link(&root, &chats, session, ISSUE, at(1)).unwrap_err();
    assert!(refused.contains("outside the project"), "{refused}");
    assert!(!refused.contains("project root"), "{refused}");
    let refused = unlink(&root, &chats, session, at(2)).unwrap_err();
    assert!(refused.contains("outside the project"), "{refused}");
    assert!(refused.contains("unlinked"), "{refused}");
}
