//! Trackers as backends (V3, FI4): what answers a work item's fields by its key.
//!
//! One work item type, many trackers. A [`Tracker`] answers the items it holds, in the neutral
//! model ([`WorkItem`]), and says nothing about a key it does not hold, so a reader asks each in
//! turn. The backends are:
//!
//! - [`Todos`]: a workspace's todos, the plane's own local tracker (`todo:` keys);
//! - the forges (`github:`, `gitlab:`), answered through the forge seam's `WorkItems` area and
//!   held in the machine-tier item cache (FI7), which is FW-7's backend. Until it lands, the items
//!   a caller already holds from a forge's answer (such as [`super::promote`]'s) are a tracker
//!   too, as a `Vec<WorkItem>`;
//! - an extension's tracker (Linear, Jira: FG-9, PE-18), behind the same trait.

use std::io;

use super::{Kind, TrackerKey, WorkItem};
use crate::workspaces::Workspace;

/// A backend that answers work items by their tracker key.
pub trait Tracker {
    /// The item `key` names, if this tracker holds it. `key` is already resolved through its
    /// aliases ([`super::log::Fold::resolve`]).
    ///
    /// **A snapshot read**: it answers what the tracker held when it was built or last
    /// refreshed, never asks the network, and cannot fail. A backend that reads a store does
    /// so up front ([`Todos::of`]); FW-7's item cache picks this shape on purpose, refreshing
    /// out of band and answering from what it holds.
    fn item(&self, key: &TrackerKey) -> Option<WorkItem>;
}

/// Items already answered, as a tracker: each answers its own key.
impl Tracker for Vec<WorkItem> {
    fn item(&self, key: &TrackerKey) -> Option<WorkItem> {
        self.iter().find(|item| &item.key == key).cloned()
    }
}

/// A workspace's open todos, as the `todo` tracker. A todo is open while its file exists, so a
/// closed one is not held and answers nothing.
#[derive(Debug, Clone)]
pub struct Todos {
    items: Vec<WorkItem>,
}

impl Todos {
    /// Workspace `ws`'s open todos, read once.
    pub fn of(ws: &Workspace) -> io::Result<Todos> {
        let items = ws
            .todos()?
            .into_iter()
            .filter_map(|todo| {
                let key = TrackerKey::todo(ws.name(), &todo.slug).ok()?;
                Some(WorkItem::new(key, Kind::Todo, todo.title))
            })
            .collect();
        Ok(Todos { items })
    }
}

impl Tracker for Todos {
    fn item(&self, key: &TrackerKey) -> Option<WorkItem> {
        self.items.item(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::State;
    use crate::workspaces::Plane;

    fn stamp(second: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 2)
            .unwrap()
            .and_hms_opt(8, 0, second)
            .unwrap()
    }

    fn stem(path: &std::path::Path) -> String {
        path.file_stem().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn a_workspaces_open_todo_is_a_work_item_of_the_todo_tracker() {
        let tmp = tempfile::tempdir().unwrap();
        let plane = Plane::open(tmp.path());
        let alpha = plane.workspace("alpha").unwrap();
        let picker = stem(&alpha.add_todo("Port the picker", stamp(0)).unwrap());
        let todos = Todos::of(&alpha).unwrap();

        let key = TrackerKey::todo("alpha", &picker).unwrap();
        let item = todos.item(&key).expect("the open todo is held");
        assert_eq!(item.key, key);
        assert_eq!(item.title, "Port the picker");
        assert_eq!(item.kind, Kind::Todo);
        assert_eq!(item.state, State::Open);
        assert_eq!(item.forge_ref, None);
        assert_eq!(item.url, "");
    }

    #[test]
    fn a_todo_tracker_holds_nothing_of_another_workspace_or_another_tracker() {
        let tmp = tempfile::tempdir().unwrap();
        let plane = Plane::open(tmp.path());
        let alpha = plane.workspace("alpha").unwrap();
        let beta = plane.workspace("beta").unwrap();
        let picker = stem(&beta.add_todo("Port the picker", stamp(0)).unwrap());
        let todos = Todos::of(&alpha).unwrap();

        assert_eq!(
            todos.item(&TrackerKey::todo("beta", &picker).unwrap()),
            None
        );
        assert_eq!(
            todos.item(&TrackerKey::todo("alpha", &picker).unwrap()),
            None
        );
        let issue = TrackerKey::parse("github:github.com/acme/api#12").unwrap();
        assert_eq!(todos.item(&issue), None);
    }

    #[test]
    fn items_already_answered_are_a_tracker_of_their_own_keys() {
        let issue = TrackerKey::parse("github:github.com/acme/api#12").unwrap();
        let epic = TrackerKey::parse("gitlab:gitlab.com/acme&3").unwrap();
        let held = vec![WorkItem::new(issue.clone(), Kind::Issue, "Port the picker")];
        assert_eq!(
            held.item(&issue).map(|i| i.title),
            Some("Port the picker".into())
        );
        assert_eq!(held.item(&epic), None);
    }
}
