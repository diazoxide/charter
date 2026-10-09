use super::*;

const DEVICE: &str = "01K6H0Z8Y3V1N3G4QK0A9T5B7C";
const OTHER_DEVICE: &str = "01K6H0Z8Y3V1N3G4QK0A9T5B7D";
const CHAT: &str = "01K6H10000AAAAAAAAAAAAAAAA";
const OTHER_CHAT: &str = "01K6H10000BBBBBBBBBBBBBBBB";

/// A project with workspaces `alpha` and `beta`, and the todo [`todo`] names open in `alpha`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for ws in ["alpha", "beta"] {
        std::fs::create_dir_all(dir.path().join("workspaces").join(ws)).unwrap();
    }
    let written = crate::workspaces::Plane::open(dir.path())
        .workspace("alpha")
        .unwrap()
        .add_todo("Port the picker", at(0).naive_utc())
        .unwrap();
    assert_eq!(
        Some(todo().todo_parts().unwrap().1),
        written.file_stem().and_then(|s| s.to_str()),
        "the todo `todo()` names is the one written"
    );
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
fn linking_a_chat_to_the_item_it_already_works_on_writes_nothing_and_another_item_replaces_it() {
    let p = project();
    let root = p.path();
    link_chat(root, Place::Workspace("alpha"), DEVICE, at(1), todo(), CHAT).unwrap();
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
    // The issue is what the todo now resolves to, so the chat already works on it.
    link_chat(
        root,
        Place::Workspace("alpha"),
        DEVICE,
        at(3),
        issue(),
        CHAT,
    )
    .unwrap();
    assert_eq!(lines_of(root, "alpha", DEVICE).len(), 2);

    // Another item is one link line, and no unlink: V3's zero or one is the fold's.
    let other = key("github:github.com/acme/api#99");
    link_chat(
        root,
        Place::Workspace("alpha"),
        DEVICE,
        at(4),
        other.clone(),
        CHAT,
    )
    .unwrap();
    let lines = lines_of(root, "alpha", DEVICE);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[2]["op"], "link");
    assert_eq!(fold(root).chat_link(CHAT), Some(other));
}

#[test]
fn unlinking_a_chat_ends_its_link_by_naming_the_item_it_resolves_to() {
    let p = project();
    let root = p.path();
    link_chat(root, Place::Workspace("alpha"), DEVICE, at(1), todo(), CHAT).unwrap();
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

    let ended = unlink_chat(root, Place::Workspace("alpha"), DEVICE, at(3), CHAT).unwrap();
    assert_eq!(ended, Some(issue()));
    let lines = lines_of(root, "alpha", DEVICE);
    assert_eq!(
        lines[2],
        serde_json::json!({
            "v": 1, "ts": "2026-10-02T08:00:03Z", "op": "unlink",
            "item": "github:github.com/acme/api#12", "chat": CHAT,
        })
    );
    assert_eq!(fold(root).chat_link(CHAT), None);
}

#[test]
fn unlinking_a_chat_that_has_no_link_writes_nothing() {
    let p = project();
    let root = p.path();
    let ended = unlink_chat(root, Place::Workspace("alpha"), DEVICE, at(1), CHAT).unwrap();
    assert_eq!(ended, None);
    assert!(!dir_for(root, "alpha").exists());
}

#[test]
fn a_chat_linked_in_one_workspace_is_unlinked_from_the_one_it_is_in_now() {
    let p = project();
    let root = p.path();
    link_chat(root, Place::Workspace("alpha"), DEVICE, at(1), todo(), CHAT).unwrap();
    let ended = unlink_chat(root, Place::Workspace("beta"), DEVICE, at(2), CHAT).unwrap();
    assert_eq!(ended, Some(todo()));
    assert_eq!(lines_of(root, "beta", DEVICE)[0]["op"], "unlink");
    let folded = fold(root);
    assert_eq!(folded.chat_link(CHAT), None);
    assert_eq!(folded.items_of("alpha"), Vec::<TrackerKey>::new());
}

#[test]
fn a_chat_at_the_project_root_cannot_be_unlinked_either() {
    let p = project();
    let root = p.path();
    let refused = unlink_chat(root, Place::ProjectRoot, DEVICE, at(1), CHAT).unwrap_err();
    assert!(refused.to_string().contains("in no workspace"), "{refused}");
    assert!(refused.to_string().contains("unlinked"), "{refused}");
}

#[test]
fn relinking_the_same_item_from_another_workspace_writes_a_line_there() {
    let p = project();
    let root = p.path();
    link_chat(
        root,
        Place::Workspace("alpha"),
        DEVICE,
        at(1),
        issue(),
        CHAT,
    )
    .unwrap();
    // The chat now stands in beta: beta's Work list shows the item only if beta's log says so.
    link_chat(root, Place::Workspace("beta"), DEVICE, at(2), issue(), CHAT).unwrap();
    assert_eq!(lines_of(root, "beta", DEVICE).len(), 1);
    let folded = fold(root);
    assert_eq!(folded.items_of("beta"), vec![issue()]);
    assert_eq!(folded.chat_link(CHAT), Some(issue()));
}

#[test]
fn a_clock_that_stepped_back_never_writes_a_ts_earlier_than_the_logs_last_line() {
    let p = project();
    let root = p.path();
    append(root, "alpha", DEVICE, at(10), &Op::link(todo())).unwrap();

    link_chat(
        root,
        Place::Workspace("alpha"),
        DEVICE,
        at(5),
        issue(),
        CHAT,
    )
    .unwrap();
    assert_eq!(
        lines_of(root, "alpha", DEVICE)[1]["ts"],
        "2026-10-02T08:00:10Z"
    );
    assert_eq!(fold(root).chat_link(CHAT), Some(issue()));
}

#[test]
fn an_unlink_from_another_workspace_is_dated_after_the_link_it_ends_whatever_the_lines_index() {
    let p = project();
    let root = p.path();
    // beta's log holds two lines before the link, so the link is its third line; alpha's log is
    // empty, so the unlink is its first. The fold orders by (ts, device file, line index,
    // workspace), so in the same second the unlink would sort first and end nothing.
    append(root, "beta", DEVICE, at(1), &Op::link(todo())).unwrap();
    append(root, "beta", DEVICE, at(2), &Op::unlink(todo())).unwrap();
    link_chat(root, Place::Workspace("beta"), DEVICE, at(5), issue(), CHAT).unwrap();

    let ended = unlink_chat(root, Place::Workspace("alpha"), DEVICE, at(5), CHAT).unwrap();
    assert_eq!(ended, Some(issue()));
    assert_eq!(
        lines_of(root, "alpha", DEVICE)[0]["ts"],
        "2026-10-02T08:00:06Z"
    );
    assert_eq!(fold(root).chat_link(CHAT), None);
}

#[test]
fn a_chat_moved_to_another_workspace_is_unlinked_there_after_its_clock_stepped_back() {
    let p = project();
    let root = p.path();
    link_chat(
        root,
        Place::Workspace("beta"),
        DEVICE,
        at(10),
        issue(),
        CHAT,
    )
    .unwrap();

    // The chat is in alpha now, and this machine's clock reads seven seconds earlier.
    let ended = unlink_chat(root, Place::Workspace("alpha"), DEVICE, at(3), CHAT).unwrap();
    assert_eq!(ended, Some(issue()));
    assert_eq!(
        lines_of(root, "alpha", DEVICE)[0]["ts"],
        "2026-10-02T08:00:11Z"
    );
    assert_eq!(fold(root).chat_link(CHAT), None);
}

#[test]
fn a_chat_is_dated_after_every_line_for_it_so_a_later_unlink_in_another_log_cannot_end_its_link() {
    let p = project();
    let root = p.path();
    let other = key("github:github.com/acme/api#99");
    // Another device's log ends a link to `other` later than this clock reads. It ends nothing
    // now; dated before it, a link to `other` would be ended by it.
    append(
        root,
        "beta",
        OTHER_DEVICE,
        at(50),
        &Op::unlink_chat(other.clone(), CHAT),
    )
    .unwrap();

    link_chat(
        root,
        Place::Workspace("alpha"),
        DEVICE,
        at(5),
        other.clone(),
        CHAT,
    )
    .unwrap();
    assert_eq!(
        lines_of(root, "alpha", DEVICE)[0]["ts"],
        "2026-10-02T08:00:51Z"
    );
    assert_eq!(fold(root).chat_link(CHAT), Some(other));
}

#[test]
fn a_link_a_synced_line_undid_is_refused_with_what_to_do() {
    let p = project();
    let root = p.path();
    let other = key("github:github.com/acme/api#99");
    // What the fold says after the write, when another device's line arrived as it was written.
    append(
        root,
        "alpha",
        DEVICE,
        at(5),
        &Op::link_chat(other.clone(), CHAT),
    )
    .unwrap();
    append(
        root,
        "beta",
        OTHER_DEVICE,
        at(6),
        &Op::unlink_chat(other.clone(), CHAT),
    )
    .unwrap();

    let said = linked_as_asked(&fold(root), CHAT, &other)
        .unwrap_err()
        .to_string();
    assert!(
        said.contains("not linked to github:github.com/acme/api#99"),
        "{said}"
    );
    assert!(said.contains("ends the link"), "{said}");
    assert!(said.contains("Try again"), "{said}");
}

#[test]
fn an_unlink_a_synced_relink_undid_says_what_the_chat_works_on_now() {
    let p = project();
    let root = p.path();
    let other = key("github:github.com/acme/api#99");
    append(
        root,
        "alpha",
        DEVICE,
        at(5),
        &Op::unlink_chat(issue(), CHAT),
    )
    .unwrap();
    append(
        root,
        "beta",
        OTHER_DEVICE,
        at(6),
        &Op::link_chat(other, CHAT),
    )
    .unwrap();

    let said = unlinked_as_asked(&fold(root), CHAT, &issue())
        .unwrap_err()
        .to_string();
    assert!(
        said.contains("another device linked it to github:github.com/acme/api#99"),
        "{said}"
    );
    assert!(!said.contains("Try again"), "{said}");
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

#[cfg(target_os = "macos")]
#[test]
fn a_line_the_filesystem_refuses_names_the_log_it_was_appending_to() {
    // #1359: an EPERM here printed only "Operation not permitted (os error 1)".
    let p = project();
    let root = p.path();
    let dir = dir_for(root, "alpha");
    std::fs::create_dir_all(&dir).unwrap();
    let _frozen = crate::rewrite::frozen::Frozen::at(&dir);

    let refused = append(root, "alpha", DEVICE, at(1), &Op::link(todo())).unwrap_err();

    crate::rewrite::frozen::names(&refused, &dir.join(format!("{DEVICE}.jsonl")));
}

#[cfg(target_os = "macos")]
#[test]
fn a_log_directory_the_filesystem_refuses_to_make_is_named() {
    let p = project();
    let root = p.path();
    let ws = root.join("workspaces").join("alpha");
    let _frozen = crate::rewrite::frozen::Frozen::at(&ws);

    let refused = append(root, "alpha", DEVICE, at(1), &Op::link(todo())).unwrap_err();

    crate::rewrite::frozen::names(&refused, &dir_for(root, "alpha"));
}

// ---- a todo key names a todo (#918) ----

#[test]
fn a_link_to_a_todo_that_does_not_exist_is_refused_and_nothing_is_written() {
    let p = project();
    let root = p.path();
    let typo = key("todo:alpha/20261002-080000-port-the-pickr");
    let refused = link_chat(
        root,
        Place::Workspace("alpha"),
        DEVICE,
        at(1),
        typo.clone(),
        CHAT,
    )
    .unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
    assert!(
        refused.to_string().contains("names no todo")
            && refused
                .to_string()
                .contains("20261002-080000-port-the-pickr"),
        "{refused}"
    );
    assert_eq!(fold(root).chat_link(CHAT), None);
    assert!(!dir_for(root, "alpha").exists(), "nothing was written");

    // The same stem in a workspace that does not hold it, or one that does not exist.
    for elsewhere in [
        "todo:beta/20261002-080000-port-the-picker",
        "todo:gamma/20261002-080000-port-the-picker",
    ] {
        let refused = link_chat(
            root,
            Place::Workspace("alpha"),
            DEVICE,
            at(1),
            key(elsewhere),
            CHAT,
        )
        .unwrap_err();
        assert!(refused.to_string().contains("names no todo"), "{refused}");
    }
    assert_eq!(fold(root).chat_link(CHAT), None);

    // The todo that is there is linked.
    link_chat(root, Place::Workspace("alpha"), DEVICE, at(2), todo(), CHAT).unwrap();
    assert_eq!(fold(root).chat_link(CHAT), Some(todo()));
}

#[test]
fn a_promoted_todo_whose_file_is_gone_still_links_as_the_item_it_became() {
    let p = project();
    let root = p.path();
    let gone = key("todo:alpha/20261001-090000-old-promoted");
    append_alias(
        root,
        "alpha",
        DEVICE,
        at(1),
        gone.clone(),
        issue(),
        Cause::Promoted,
    )
    .unwrap();

    link_chat(root, Place::Workspace("alpha"), DEVICE, at(2), gone, CHAT).unwrap();
    assert_eq!(fold(root).chat_link(CHAT), Some(issue()));
}

#[test]
fn an_alias_to_a_todo_that_does_not_exist_is_refused_naming_both() {
    let p = project();
    let root = p.path();
    // A workspace rename's alias, to a todo the new workspace never got.
    let from = key("todo:old/20261001-090000-never-written");
    let to = key("todo:alpha/20261001-090000-never-written");
    append_alias(
        root,
        "alpha",
        DEVICE,
        at(1),
        from.clone(),
        to.clone(),
        Cause::Renamed,
    )
    .unwrap();

    let refused = link_chat(
        root,
        Place::Workspace("alpha"),
        DEVICE,
        at(2),
        from.clone(),
        CHAT,
    )
    .unwrap_err();
    let said = refused.to_string();
    assert!(
        said.contains(to.as_str()) && said.contains(from.as_str()),
        "{said}"
    );
    assert_eq!(fold(root).chat_link(CHAT), None);
}
