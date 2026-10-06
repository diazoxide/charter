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
    /// The tracker's own type for the item, by its name: a GitHub organisation's issue type
    /// (`Bug`, `Feature`), GitLab's work item type. It is carried beside [`Kind`], never mapped
    /// onto it: a tracker's types are its owner's to name, and `Kind` is only what charter
    /// draws differently.
    pub issue_type: Option<String>,
    pub state: State,
    /// Why a closed item was closed, when its tracker says. `None` while it is open. A tracker
    /// may say it where [`crate::forge::Capability::CloseReasons`] is not known to be there (a
    /// self-managed GitLab still marks a duplicate): show it only where that capability's answer
    /// does not take the `Hidden` fallback.
    pub closed_as: Option<ClosedAs>,
    /// The item's status, as its tracker names it (`In progress`, `Won't do`). An item-level
    /// status of the tracker's own (GitLab's work item status, a Jira or Linear status) comes
    /// first; with none, it is the first board's in [`WorkItem::placements`] that gives one, as
    /// [`WorkItem::iteration`] is. GitHub keeps a status per board only, so there it is always a
    /// board's ([`crate::forge::Capability::ItemStatus`]).
    pub status: Option<String>,
    pub milestone: Option<Milestone>,
    /// The iteration (sprint) the item is planned in. On GitHub it is a Projects v2 board's
    /// iteration field: each [`Placement`] keeps its own, and this is the first board's, in the
    /// tracker's order, that gives one. On GitLab it is the item's own (a Premium feature).
    pub iteration: Option<Iteration>,
    /// The tracker's own boards that hold the item, and its status on each: GitHub's Projects
    /// v2, a GitLab repo's issue boards. A board here is the tracker's, never charter's own
    /// [`board::Board`], which is a view.
    pub placements: Vec<Placement>,
    /// Label names, as the tracker spells them.
    pub labels: Vec<String>,
    /// Assignees by the tracker's own login.
    pub assignees: Vec<String>,
    /// How this item stands to others (CONTEXT.md, **Relation**).
    pub relations: Vec<Relation>,
}

impl WorkItem {
    /// An item of `kind` with only its key and title known: open, with no forge id, page,
    /// type, status, milestone, iteration, board, label, assignee or relation. A backend fills
    /// in the rest.
    pub fn new(key: TrackerKey, kind: Kind, title: impl Into<String>) -> WorkItem {
        WorkItem {
            key,
            forge_ref: None,
            url: String::new(),
            title: title.into(),
            kind,
            issue_type: None,
            state: State::Open,
            closed_as: None,
            status: None,
            milestone: None,
            iteration: None,
            placements: Vec::new(),
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

/// Why an item was closed, as GitHub's `state_reason` says it. A tracker that keeps no reason
/// answers none ([`crate::forge::Capability::CloseReasons`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClosedAs {
    /// The work was done.
    Completed,
    /// It will not be done.
    NotPlanned,
    /// Another item holds it.
    Duplicate,
}

impl ClosedAs {
    /// A forge's reason word, in any case. A word that is no close reason (`reopened`, which
    /// GitHub gives an open item) or one charter does not know is none.
    pub fn of_forge(word: &str) -> Option<ClosedAs> {
        match word.to_ascii_lowercase().as_str() {
            "completed" => Some(ClosedAs::Completed),
            "not_planned" => Some(ClosedAs::NotPlanned),
            "duplicate" => Some(ClosedAs::Duplicate),
            _ => None,
        }
    }
}

/// An iteration, a sprint: a span of days the item is planned in. Its id on the forge travels
/// beside its title, as a milestone's does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iteration {
    pub forge_ref: Option<ForgeRef>,
    /// Its title. A GitLab cadence's iterations have none: they are named by their dates.
    pub title: Option<String>,
    /// Its first day.
    pub start: Option<chrono::NaiveDate>,
    /// Its last day, inclusive.
    pub end: Option<chrono::NaiveDate>,
}

impl Iteration {
    /// An iteration from a forge that gives its first and last days, as GitLab does
    /// (`start_date`, `due_date`). Each is read by its date alone; one that is not a date is
    /// none.
    pub fn of_forge(
        forge_ref: Option<ForgeRef>,
        title: Option<&str>,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Iteration {
        Iteration {
            forge_ref,
            title: title.map(str::to_string),
            start: start.and_then(day_of),
            end: end.and_then(day_of),
        }
    }

    /// An iteration from a forge that gives its first day and how many days it lasts, as a
    /// GitHub Projects v2 iteration field does (`startDate`, `duration`).
    pub fn lasting(
        forge_ref: Option<ForgeRef>,
        title: Option<&str>,
        start: &str,
        days: u32,
    ) -> Iteration {
        let start = day_of(start);
        let end = start.and_then(|first| {
            let more = days.checked_sub(1)?;
            first.checked_add_days(chrono::Days::new(u64::from(more)))
        });
        Iteration {
            forge_ref,
            title: title.map(str::to_string),
            start,
            end,
        }
    }
}

/// One of the tracker's own boards that holds an item, and the item's status and iteration
/// there: a GitHub Projects v2 board, its `Status` field and its iteration field. FW-9 may group
/// charter's board by the status.
///
/// This status is per board. GitLab, Jira and Linear give an item one status of its own, which
/// [`WorkItem::status`] holds beside this one. On GitLab a board is a repo's issue board, and the
/// status there is the label of the first label list holding the item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The board's id on the forge.
    pub board: ForgeRef,
    pub board_title: String,
    pub board_url: String,
    /// The item's status on that board, as the board names the option; `None` when it has
    /// none set, or the board has no status field.
    pub status: Option<String>,
    /// The item's iteration on that board, when the board has an iteration field set for it.
    pub iteration: Option<Iteration>,
}

/// A day from a forge's date or timestamp, read by its first ten characters.
fn day_of(text: &str) -> Option<chrono::NaiveDate> {
    let day = text.get(..10)?;
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
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
        let due = due.and_then(day_of);
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
    fn a_close_reason_is_read_from_a_forges_word_in_any_case() {
        assert_eq!(ClosedAs::of_forge("completed"), Some(ClosedAs::Completed));
        assert_eq!(
            ClosedAs::of_forge("NOT_PLANNED"),
            Some(ClosedAs::NotPlanned)
        );
        assert_eq!(ClosedAs::of_forge("duplicate"), Some(ClosedAs::Duplicate));
        for none in ["reopened", "", "won't fix"] {
            assert_eq!(ClosedAs::of_forge(none), None, "{none:?}");
        }
    }

    #[test]
    fn an_iteration_that_lasts_fourteen_days_ends_on_its_fourteenth() {
        let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 10, d);
        let sprint = Iteration::lasting(None, Some("Sprint 3"), "2026-10-05", 14);
        assert_eq!((sprint.start, sprint.end), (day(5), day(18)));
        let gitlab = Iteration::of_forge(
            None,
            Some("Sprint 3"),
            Some("2026-10-05"),
            Some("2026-10-18"),
        );
        assert_eq!(gitlab, sprint);
        let undated = Iteration::lasting(None, Some("Sprint 3"), "soon", 14);
        assert_eq!((undated.start, undated.end), (None, None));
        assert_eq!(
            Iteration::lasting(None, Some("x"), "2026-10-05", 0).end,
            None
        );
        // A GitLab cadence names its iterations by their dates and gives them no title.
        let cadence = Iteration::of_forge(None, None, Some("2026-10-05"), Some("2026-10-18"));
        assert_eq!((cadence.title, cadence.start), (None, day(5)));
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
