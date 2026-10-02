//! A workspace's Work list: one item per tracker key, its aliases collapsed (ADR 0088 §3, §5).
//!
//! What it lists is the workspace's open todos, as the `todo` tracker's items, and every item
//! the work link log says the workspace or one of its chats holds. Every key is read through its
//! aliases first, so **a todo that was promoted is not listed**: the issue it became is, with the
//! todo's chats under it. FI6's layer 1, every issue of the workspace's repos, joins the list
//! from the item cache (FW-7); FW-9 draws it as a board.

use std::collections::BTreeSet;
use std::io;

use super::TrackerKey;
use super::log::Fold;
use crate::workspaces::Workspace;

/// One row of the Work list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The item's key, resolved through its aliases.
    pub key: TrackerKey,
    /// An open todo's title. A forge item's title is the item cache's (FW-7), never the
    /// project's, so it is `None` here.
    pub todo: Option<String>,
    /// The chats linked to it, by ULID.
    pub chats: Vec<String>,
}

/// Workspace `ws`'s Work list: its open todos oldest first, then every other item it holds, each
/// key once.
pub fn of(ws: &Workspace, folded: &Fold) -> io::Result<Vec<Listed>> {
    let mut seen: BTreeSet<TrackerKey> = BTreeSet::new();
    let mut out = Vec::new();
    let mut add = |key: TrackerKey, todo: Option<String>, out: &mut Vec<Listed>| {
        let key = folded.resolve(&key);
        if seen.insert(key.clone()) {
            let todo = if key.tracker() == "todo" { todo } else { None };
            let chats = folded.chats_on(&key);
            out.push(Listed { key, todo, chats });
        }
    };
    for todo in ws.todos()? {
        let Ok(key) = TrackerKey::todo(ws.name(), &todo.slug) else {
            continue;
        };
        add(key, Some(todo.title), &mut out);
    }
    for key in folded.items_of(ws.name()) {
        add(key, None, &mut out);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::log::{self, Cause, Op};
    use crate::workspaces::Plane;

    const DEVICE: &str = "01K6H0Z8Y3V1N3G4QK0A9T5B7C";
    const CHAT: &str = "01K6H10000AAAAAAAAAAAAAAAA";

    fn stamp(second: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 2)
            .unwrap()
            .and_hms_opt(8, 0, second)
            .unwrap()
    }

    fn at(second: u32) -> chrono::DateTime<chrono::Utc> {
        stamp(second).and_utc()
    }

    #[test]
    fn a_promoted_todo_is_one_item_the_issue_with_the_todos_chat_under_it() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = Plane::open(tmp.path()).workspace("alpha").unwrap();
        let picker = ws.add_todo("Port the picker", stamp(0)).unwrap();
        ws.add_todo("Write the migration", stamp(1)).unwrap();
        let stem = picker.file_stem().unwrap().to_string_lossy().into_owned();
        let todo = TrackerKey::todo("alpha", &stem).unwrap();
        let issue = TrackerKey::parse("github:github.com/acme/api#12").unwrap();
        log::append(
            tmp.path(),
            "alpha",
            DEVICE,
            at(2),
            &Op::link_chat(todo.clone(), CHAT),
        )
        .unwrap();

        let before = of(&ws, &log::fold(tmp.path())).unwrap();
        assert_eq!(before.len(), 2);
        assert_eq!(before[0].key, todo);
        assert_eq!(before[0].todo.as_deref(), Some("Port the picker"));
        assert_eq!(before[0].chats, [CHAT]);

        // Promoted, but the todo file not yet closed: readers already treat it as the issue.
        log::append_alias(
            tmp.path(),
            "alpha",
            DEVICE,
            at(3),
            todo,
            issue.clone(),
            Cause::Promoted,
        )
        .unwrap();
        let after = of(&ws, &log::fold(tmp.path())).unwrap();
        let keys: Vec<&str> = after.iter().map(|l| l.key.as_str()).collect();
        assert_eq!(
            keys.len(),
            2,
            "one card for the promoted todo, not two: {keys:?}"
        );
        assert_eq!(after[0].key, issue);
        assert_eq!(after[0].todo, None);
        assert_eq!(after[0].chats, [CHAT], "the chat link reaches the issue");
        assert_eq!(after[1].todo.as_deref(), Some("Write the migration"));
    }

    #[test]
    fn an_item_the_workspace_links_and_one_of_its_chats_links_is_listed_once() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = Plane::open(tmp.path()).workspace("alpha").unwrap();
        let issue = TrackerKey::parse("gitlab:gitlab.com/acme/sub/api#4").unwrap();
        log::append(tmp.path(), "alpha", DEVICE, at(1), &Op::link(issue.clone())).unwrap();
        log::append(
            tmp.path(),
            "alpha",
            DEVICE,
            at(2),
            &Op::link_chat(issue.clone(), CHAT),
        )
        .unwrap();
        let listed = of(&ws, &log::fold(tmp.path())).unwrap();
        assert_eq!(
            listed,
            [Listed {
                key: issue,
                todo: None,
                chats: vec![CHAT.to_string()]
            }]
        );
    }
}
