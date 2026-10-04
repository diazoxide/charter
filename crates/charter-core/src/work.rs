//! Work items: one type, with trackers as its backends (V3, FI4, ADR 0088).
//!
//! A work item is a view of a **Workspace**, not a concept of its own (X22, FI5). It is named by
//! its [`key::TrackerKey`]: the tracker's name and the item's own reference where it lives. A
//! forge's own identifier ([`crate::forge::backend::ForgeRef`]) travels beside the key in
//! [`WorkItem`], never inside it, and never into the project.
//!
//! - [`board`]: a workspace's board, one card per item, derived and never stored.
//! - [`key`]: the key's grammar, normalised once and compared exactly.
//! - [`log`]: the work link log, `workspaces/<ws>/work/<device>.jsonl`, and its fold.
//! - [`list`]: a workspace's Work list, one item per key, its aliases collapsed.
//! - [`promote`]: a todo promoted to an issue, which closes the todo and leaves an alias.
//! - [`tracker`]: the trackers as backends, which answer an item's fields by its key.

pub mod board;
pub mod key;
pub mod list;
pub mod log;
pub mod promote;
pub mod tracker;

pub use key::TrackerKey;
pub use tracker::Tracker;

use crate::forge::backend::ForgeRef;
use crate::forge::pr::Pr;

/// One work item as a tracker answers it, in the neutral model (FI4): its key and the tracker's
/// own identifier beside it, then the fields every backend maps its own onto.
///
/// The key is what the project and the audit name the item by. The [`ForgeRef`] is the forge's
/// node or global id, which a round trip uses so that it never re-derives one (ADR 0070 §1). A
/// todo has none.
///
/// **Content, never project state.** The fields are what the tracker says now. They live in the
/// tracker and in the machine-tier item cache (FI7, FW-7), and the work link log never holds them
/// (ADR 0088 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    pub key: TrackerKey,
    pub forge_ref: Option<ForgeRef>,
    /// The item's page, for a person to open. Empty for a todo.
    pub url: String,
    pub title: String,
    pub kind: Kind,
    pub state: State,
    pub milestone: Option<Milestone>,
    /// Label names, as the tracker spells them.
    pub labels: Vec<String>,
    /// Assignees by the tracker's own login.
    pub assignees: Vec<String>,
    /// How this item stands to others (CONTEXT.md, **Relation**).
    pub relations: Vec<Relation>,
}

impl WorkItem {
    /// An item of `kind` with only its key and title known: open, with no forge id, page,
    /// milestone, label, assignee or relation. A backend fills in the rest.
    pub fn new(key: TrackerKey, kind: Kind, title: impl Into<String>) -> WorkItem {
        WorkItem {
            key,
            forge_ref: None,
            url: String::new(),
            title: title.into(),
            kind,
            state: State::Open,
            milestone: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            relations: Vec::new(),
        }
    }

    /// A sub-issue is an issue with a parent (FI4): GitHub's sub-issue, GitLab's child item.
    pub fn is_sub_issue(&self) -> bool {
        self.kind == Kind::Issue
            && self
                .relations
                .iter()
                .any(|r| matches!(r, Relation::Parent(_)))
    }
}

/// What kind of work item it is. A sub-issue is not a kind of its own: it is an
/// [`Kind::Issue`] with a [`Relation::Parent`] ([`WorkItem::is_sub_issue`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A todo, the plane's own tracker's item.
    Todo,
    /// A forge issue, GitLab task or an extension tracker's ticket.
    Issue,
    /// An epic: GitLab's group epic. GitHub has none (a capability flag, FW-6a).
    Epic,
}

/// Whether the item is still open. Each tracker's own words (`open`, `opened`, `closed`, a
/// closed todo being a deleted file) map onto these two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    Open,
    Closed,
}

impl State {
    /// A forge's state word, in any case (REST says `closed`, GraphQL `CLOSED`): `closed` is
    /// closed, anything else (`open`, `opened`, `OPEN`, `locked`) is open, so an item a forge
    /// answers is never dropped from a board for a word charter does not know.
    pub fn of_forge(word: &str) -> State {
        if word.eq_ignore_ascii_case("closed") {
            State::Closed
        } else {
            State::Open
        }
    }
}

/// A milestone, as the item names it. The forge's own id for it travels beside its title, as an
/// item's [`ForgeRef`] does, so two milestones that share a title in different repos or groups
/// are told apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone {
    /// The forge's id for it: GitHub's node id, GitLab's instance-wide id. `None` when the
    /// answer named none.
    pub forge_ref: Option<ForgeRef>,
    pub title: String,
    /// Its due date, when it has one.
    pub due: Option<chrono::NaiveDate>,
}

impl Milestone {
    /// A milestone from a forge's answer. `due` is read by its date alone, so GitHub's
    /// `due_on` timestamp (`2026-10-31T07:00:00Z`) and GitLab's `due_date` (`2026-10-31`) give
    /// the same day. A due date that is not one is no due date.
    pub fn of_forge(
        forge_ref: Option<ForgeRef>,
        title: impl Into<String>,
        due: Option<&str>,
    ) -> Milestone {
        let due = due.and_then(|text| {
            let day = text.get(..10)?;
            chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
        });
        Milestone {
            forge_ref,
            title: title.into(),
            due,
        }
    }
}

/// How a work item stands to another (ADR 0088 §6). "Work link" is the log's word and "link"
/// alone the runner link's, so these are relations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relation {
    /// A pull or merge request that closes this item when it merges.
    ClosedBy(Pr),
    /// This item cannot finish before that one.
    BlockedBy(TrackerKey),
    /// This item holds up that one.
    Blocks(TrackerKey),
    /// This item sits under that one: a sub-issue's issue, an issue's epic.
    Parent(TrackerKey),
    /// That item sits under this one.
    Child(TrackerKey),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> TrackerKey {
        TrackerKey::parse(text).unwrap()
    }

    #[test]
    fn a_sub_issue_is_an_issue_with_a_parent() {
        let mut child = WorkItem::new(key("github:github.com/acme/api#13"), Kind::Issue, "Child");
        assert!(!child.is_sub_issue());
        child
            .relations
            .push(Relation::BlockedBy(key("github:github.com/acme/api#11")));
        assert!(!child.is_sub_issue(), "blocked-by is not a parent");
        child
            .relations
            .push(Relation::Parent(key("github:github.com/acme/api#12")));
        assert!(child.is_sub_issue());

        let mut epic = WorkItem::new(key("gitlab:gitlab.com/acme&4"), Kind::Epic, "Epic");
        epic.relations
            .push(Relation::Parent(key("gitlab:gitlab.com/acme&3")));
        assert!(
            !epic.is_sub_issue(),
            "an epic under an epic is still an epic"
        );
    }

    #[test]
    fn closed_is_the_only_forge_word_that_closes_an_item() {
        for closed in ["closed", "CLOSED"] {
            assert_eq!(State::of_forge(closed), State::Closed, "{closed:?}");
        }
        for open in ["open", "opened", "OPEN", "locked", ""] {
            assert_eq!(State::of_forge(open), State::Open, "{open:?}");
        }
    }

    #[test]
    fn a_milestones_due_date_is_its_day_on_either_forge() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 31);
        let github = Milestone::of_forge(
            Some(ForgeRef("MI_kwDOAcme1".into())),
            "v1",
            Some("2026-10-31T07:00:00Z"),
        );
        assert_eq!(github.due, day);
        assert_eq!(github.forge_ref, Some(ForgeRef("MI_kwDOAcme1".into())));
        assert_eq!(Milestone::of_forge(None, "v1", Some("2026-10-31")).due, day);
        assert_eq!(Milestone::of_forge(None, "v1", None).due, None);
        assert_eq!(Milestone::of_forge(None, "v1", Some("soon")).due, None);
    }
}
