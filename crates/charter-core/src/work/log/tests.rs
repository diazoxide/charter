use super::*;

const DEVICE: &str = "01K6H0Z8Y3V1N3G4QK0A9T5B7C";
const OTHER_DEVICE: &str = "01K6H0Z8Y3V1N3G4QK0A9T5B7D";
const CHAT: &str = "01K6H10000AAAAAAAAAAAAAAAA";
const OTHER_CHAT: &str = "01K6H10000BBBBBBBBBBBBBBBB";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for ws in ["alpha", "beta"] {
        std::fs::create_dir_all(dir.path().join("workspaces").join(ws)).unwrap();
    }
    dir
}

fn key(text: &str) -> TrackerKey {
    TrackerKey::parse(text).unwrap()
}

fn at(second: u32) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2026, 10, 2, 8, 0, second)
        .unwrap()
}

fn todo() -> TrackerKey {
    key("todo:alpha/20261002-080000-port-the-picker")
}

fn issue() -> TrackerKey {
    key("github:github.com/acme/api#12")
}

fn lines_of(root: &Path, ws: &str, device: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir_for(root, ws).join(format!("{device}.jsonl")))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn each_line_is_one_of_four_closed_key_sets_holding_keys_and_chat_ids_only() {
    let p = project();
    let root = p.path();
    append(root, "alpha", DEVICE, at(1), &Op::link(todo())).unwrap();
    append(root, "alpha", DEVICE, at(2), &Op::link_chat(todo(), CHAT)).unwrap();
    append(root, "alpha", DEVICE, at(3), &Op::unlink_chat(todo(), CHAT)).unwrap();
    append(root, "alpha", DEVICE, at(4), &Op::unlink(todo())).unwrap();
    append_alias(
        root,
        "alpha",
        DEVICE,
        at(5),
        todo(),
        issue(),
        Cause::Promoted,
    )
    .unwrap();
    let todo = todo().to_string();
    assert_eq!(
        lines_of(root, "alpha", DEVICE),
        [
            serde_json::json!({"v": 1, "ts": "2026-10-02T08:00:01Z", "op": "link", "item": todo}),
            serde_json::json!({"v": 1, "ts": "2026-10-02T08:00:02Z", "op": "link", "item": todo,
                               "chat": CHAT}),
            serde_json::json!({"v": 1, "ts": "2026-10-02T08:00:03Z", "op": "unlink", "item": todo,
                               "chat": CHAT}),
            serde_json::json!({"v": 1, "ts": "2026-10-02T08:00:04Z", "op": "unlink", "item": todo}),
            serde_json::json!({"v": 1, "ts": "2026-10-02T08:00:05Z", "op": "alias", "from": todo,
                               "to": "github:github.com/acme/api#12", "cause": "promoted"}),
        ]
    );
}

#[test]
fn the_log_is_one_file_per_device_named_by_its_device_id_and_never_a_hostname() {
    let p = project();
    assert!(append(p.path(), "alpha", "my-laptop", at(1), &Op::link(todo())).is_err());
    assert!(
        append(
            p.path(),
            "alpha",
            DEVICE,
            at(1),
            &Op::link_chat(todo(), "chat-3")
        )
        .is_err(),
        "a chat is named by its ULID, never its number"
    );
    assert!(!dir_for(p.path(), "alpha").exists(), "nothing was written");
}

#[test]
fn a_chats_link_is_the_last_line_for_it_across_every_workspaces_log() {
    let p = project();
    let root = p.path();
    let other = key("github:github.com/acme/api#13");
    append(root, "alpha", DEVICE, at(1), &Op::link_chat(todo(), CHAT)).unwrap();
    // The chat moved to beta and was linked again there, on another device: it keeps one link.
    append(
        root,
        "beta",
        OTHER_DEVICE,
        at(2),
        &Op::link_chat(other.clone(), CHAT),
    )
    .unwrap();
    let folded = fold(root);
    assert_eq!(folded.chat_link(CHAT), Some(other.clone()));
    assert_eq!(folded.chats_on(&todo()), Vec::<String>::new());
    assert_eq!(folded.chats_on(&other), [CHAT]);

    append(
        root,
        "beta",
        OTHER_DEVICE,
        at(3),
        &Op::unlink_chat(other.clone(), CHAT),
    )
    .unwrap();
    assert_eq!(
        fold(root).chat_link(CHAT),
        None,
        "an unlink is a line, not an absence"
    );
}

#[test]
fn lines_are_folded_by_time_then_file_name_then_line_and_not_by_where_they_sit() {
    let p = project();
    let root = p.path();
    let later = key("github:github.com/acme/api#13");
    // OTHER_DEVICE's file sorts after DEVICE's, but its line is older.
    append(
        root,
        "alpha",
        OTHER_DEVICE,
        at(1),
        &Op::link_chat(todo(), CHAT),
    )
    .unwrap();
    append(
        root,
        "alpha",
        DEVICE,
        at(2),
        &Op::link_chat(later.clone(), CHAT),
    )
    .unwrap();
    assert_eq!(fold(root).chat_link(CHAT), Some(later.clone()));
    // At the same second, the file name decides.
    append(
        root,
        "alpha",
        DEVICE,
        at(5),
        &Op::link_chat(todo(), OTHER_CHAT),
    )
    .unwrap();
    append(
        root,
        "alpha",
        OTHER_DEVICE,
        at(5),
        &Op::link_chat(later.clone(), OTHER_CHAT),
    )
    .unwrap();
    assert_eq!(fold(root).chat_link(OTHER_CHAT), Some(later));
}

#[test]
fn a_key_is_read_through_its_aliases_before_anything_is_compared() {
    let p = project();
    let root = p.path();
    let moved = key("github:github.com/acme/platform#12");
    append(root, "alpha", DEVICE, at(1), &Op::link_chat(todo(), CHAT)).unwrap();
    append_alias(
        root,
        "alpha",
        DEVICE,
        at(2),
        todo(),
        issue(),
        Cause::Promoted,
    )
    .unwrap();
    append_alias(
        root,
        "beta",
        OTHER_DEVICE,
        at(3),
        issue(),
        moved.clone(),
        Cause::Moved,
    )
    .unwrap();
    let folded = fold(root);
    assert_eq!(folded.resolve(&todo()), moved);
    assert_eq!(folded.chat_link(CHAT), Some(moved.clone()));
    assert_eq!(folded.chats_on(&moved), [CHAT]);
    assert!(folded.is_aliased(&todo()) && !folded.is_aliased(&moved));
}

#[test]
fn an_alias_that_would_close_a_cycle_is_refused_when_it_is_written() {
    let p = project();
    let root = p.path();
    append_alias(
        root,
        "alpha",
        DEVICE,
        at(1),
        todo(),
        issue(),
        Cause::Promoted,
    )
    .unwrap();
    assert!(append_alias(root, "alpha", DEVICE, at(2), issue(), todo(), Cause::Moved).is_err());
    assert!(append_alias(root, "alpha", DEVICE, at(2), issue(), issue(), Cause::Moved).is_err());
    assert_eq!(lines_of(root, "alpha", DEVICE).len(), 1);
}

#[test]
fn a_cycle_a_merge_made_stops_at_the_first_repeated_key_and_is_reported() {
    let p = project();
    let root = p.path();
    // Two devices, each refusing nothing on its own, merged.
    append_alias(
        root,
        "alpha",
        DEVICE,
        at(1),
        todo(),
        issue(),
        Cause::Promoted,
    )
    .unwrap();
    // The other device wrote its line before it had seen this one's, and the merge kept both.
    let theirs = dir_for(root, "beta");
    std::fs::create_dir_all(&theirs).unwrap();
    std::fs::write(
        theirs.join(format!("{OTHER_DEVICE}.jsonl")),
        format!(
            "{{\"v\":1,\"ts\":\"2026-10-02T08:00:02Z\",\"op\":\"alias\",\"from\":\"{}\",\
             \"to\":\"{}\",\"cause\":\"moved\"}}\n",
            issue(),
            todo()
        ),
    )
    .unwrap();
    let folded = fold(root);
    assert_eq!(folded.skipped(), 0);
    assert_eq!(folded.resolve(&todo()), issue());
    assert_eq!(folded.resolve(&issue()), todo());
    assert_eq!(folded.cycles(), [issue(), todo()]);
}

#[test]
fn a_line_with_any_other_key_set_op_or_cause_is_skipped_and_counted() {
    let p = project();
    let root = p.path();
    append(root, "alpha", DEVICE, at(1), &Op::link_chat(todo(), CHAT)).unwrap();
    let file = dir_for(root, "alpha").join(format!("{DEVICE}.jsonl"));
    let mut text = std::fs::read_to_string(&file).unwrap();
    for bad in [
        r#"{"v":1,"ts":"2026-10-02T08:00:09Z","op":"link","item":"todo:alpha/x","title":"leak"}"#,
        r#"{"v":2,"ts":"2026-10-02T08:00:09Z","op":"link","item":"todo:alpha/x"}"#,
        r#"{"v":1,"ts":"2026-10-02T08:00:09Z","op":"claim","item":"todo:alpha/x"}"#,
        r#"{"v":1,"ts":"2026-10-02T08:00:09Z","op":"alias","from":"todo:alpha/x","to":"todo:alpha/y","cause":"guessed"}"#,
        r#"{"v":1,"ts":"yesterday","op":"link","item":"todo:alpha/x"}"#,
        r#"{"v":1,"ts":"2026-10-02T08:00:09Z","op":"link","item":"not a key"}"#,
        r#"{"v":1,"ts":"2026-10-02T08:00:09Z","op":"unlink","item":"todo:alpha/x","chat":"3"}"#,
        r#"{"v":1,"ts":"2026-10-02T08:00:09Z","op":"link","item":"todo:alpha/x","chat":"01K6H10000AAAAAAAAAAAAAAAA","forge_ref":"I_1"}"#,
        r#"{"v":1,"ts":"2026-10-02T08:00:09Z","op":"link","item":"todo:alpha/x"#,
        "",
    ] {
        text.push_str(bad);
        text.push('\n');
    }
    std::fs::write(&file, text).unwrap();
    let folded = fold(root);
    assert_eq!(folded.skipped(), 9, "the blank line is not a line");
    assert_eq!(
        folded.skipped_in().iter().collect::<Vec<_>>(),
        [(&format!("alpha/{DEVICE}.jsonl"), &9)]
    );
    assert_eq!(folded.chat_link(CHAT), Some(todo()));
    assert!(folded.items_of("alpha").contains(&todo()));
}

#[test]
fn a_workspaces_items_are_its_workspace_links_and_the_items_its_chats_are_linked_to() {
    let p = project();
    let root = p.path();
    let held = key("gitlab:gitlab.com/acme/sub/api#4");
    append(root, "alpha", DEVICE, at(1), &Op::link(held.clone())).unwrap();
    append(root, "alpha", DEVICE, at(2), &Op::link_chat(todo(), CHAT)).unwrap();
    append(root, "beta", DEVICE, at(3), &Op::link(issue())).unwrap();
    let folded = fold(root);
    assert_eq!(folded.items_of("alpha"), [held.clone(), todo()]);
    assert_eq!(folded.items_of("beta"), [issue()]);

    append(root, "alpha", DEVICE, at(4), &Op::unlink(held)).unwrap();
    append(root, "alpha", DEVICE, at(5), &Op::unlink_chat(todo(), CHAT)).unwrap();
    assert_eq!(fold(root).items_of("alpha"), Vec::<TrackerKey>::new());
}

#[test]
fn a_project_with_no_work_links_folds_to_nothing() {
    let p = project();
    let folded = fold(p.path());
    assert_eq!(folded.skipped(), 0);
    assert_eq!(folded.items_of("alpha"), Vec::<TrackerKey>::new());
    assert_eq!(folded.resolve(&todo()), todo());
}

#[test]
fn a_chat_at_the_project_root_cannot_be_linked_and_one_in_a_workspace_can() {
    let p = project();
    let root = p.path();
    let refused = link_chat(root, Place::ProjectRoot, DEVICE, at(1), todo(), CHAT).unwrap_err();
    assert!(refused.to_string().contains("in no workspace"), "{refused}");
    assert_eq!(fold(root).chat_link(CHAT), None);
    assert!(!root.join("work").exists() && !dir_for(root, "alpha").exists());

    link_chat(root, Place::Workspace("alpha"), DEVICE, at(2), todo(), CHAT).unwrap();
    assert_eq!(fold(root).chat_link(CHAT), Some(todo()));
}

#[test]
fn an_unlink_ends_the_link_whose_item_it_names_after_both_are_resolved() {
    let p = project();
    let root = p.path();
    append(root, "alpha", DEVICE, at(1), &Op::link(todo())).unwrap();
    append(root, "alpha", DEVICE, at(2), &Op::link_chat(todo(), CHAT)).unwrap();
    append_alias(
        root,
        "alpha",
        DEVICE,
        at(3),
        todo(),
        issue(),
        Cause::Promoted,
    )
    .unwrap();
    // A chat unlink naming some other item ends nothing.
    let other = key("github:github.com/acme/api#99");
    append(root, "alpha", DEVICE, at(4), &Op::unlink_chat(other, CHAT)).unwrap();
    assert_eq!(fold(root).chat_link(CHAT), Some(issue()));
    // An unlink naming the todo, written after it was promoted, ends the links too: both sides
    // are resolved before they are compared.
    append(root, "alpha", DEVICE, at(5), &Op::unlink(todo())).unwrap();
    append(
        root,
        "alpha",
        DEVICE,
        at(6),
        &Op::unlink_chat(issue(), CHAT),
    )
    .unwrap();
    let folded = fold(root);
    assert_eq!(folded.chat_link(CHAT), None);
    assert_eq!(folded.items_of("alpha"), Vec::<TrackerKey>::new());
}

#[cfg(unix)]
#[test]
fn a_log_that_resolves_out_of_the_project_is_neither_read_nor_written() {
    let p = project();
    let root = p.path();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join(format!("{DEVICE}.jsonl")),
        format!(
            "{{\"v\":1,\"ts\":\"2026-10-02T08:00:01Z\",\"op\":\"link\",\"item\":\"{}\",\
             \"chat\":\"{CHAT}\"}}\n",
            todo()
        ),
    )
    .unwrap();
    std::os::unix::fs::symlink(outside.path(), dir_for(root, "alpha")).unwrap();
    assert_eq!(
        fold(root).chat_link(CHAT),
        None,
        "not read through the link"
    );
    assert!(check_writable(root, "alpha", DEVICE).is_err());
    assert!(append(root, "alpha", DEVICE, at(2), &Op::link(todo())).is_err());
}
