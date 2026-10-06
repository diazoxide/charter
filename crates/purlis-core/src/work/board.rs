//! A workspace's board: its Work list drawn as cards, one per work item (FI4's Board, a view).
//!
//! A board is a **view**, never a store (FI5, FI7): it is derived each time from the Work list
//! ([`super::list`], one row per key with its aliases collapsed) and from the trackers that
//! answer each key's fields ([`super::tracker`]). So a todo that was promoted is one card, the
//! issue's, with the todo's chats on it, and building the board twice gives the same board and
//! writes nothing. FW-9 draws it, with its own grouping, filters and milestone view on top.

use std::io;

use super::log::Fold;
use super::tracker::Tracker;
use super::{Kind, State, TrackerKey, WorkItem, list};
use crate::workspaces::Workspace;

/// The board's default grouping: what is still to do, and what is done. FW-9 may group by
/// something else (a milestone, a Projects v2 status field) and replace it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Open,
    Closed,
}

/// One card: one work item, with the chats that work on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// The item's key, resolved through its aliases.
    pub key: TrackerKey,
    /// The item as a tracker answered it, or `None` when no tracker the board was given holds
    /// it yet: a forge item the item cache (FW-7) has not read. The card still stands, by its
    /// key, because the workspace or a chat links to it.
    pub item: Option<WorkItem>,
    /// The chats linked to it, by ULID.
    pub chats: Vec<String>,
}

impl Card {
    /// The column it stands in. An item no tracker answered stands in [`Column::Open`]: a link
    /// to it is live work until a tracker says it is closed.
    pub fn column(&self) -> Column {
        match self.item.as_ref().map(|item| item.state) {
            Some(State::Closed) => Column::Closed,
            Some(State::Open) | None => Column::Open,
        }
    }
}

/// A workspace's board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    /// Every card, in the Work list's order: open todos oldest first, then the other items.
    pub cards: Vec<Card>,
}

impl Board {
    /// The cards standing in `column`, in board order.
    pub fn column(&self, column: Column) -> impl Iterator<Item = &Card> {
        self.cards
            .iter()
            .filter(move |card| card.column() == column)
    }
}

/// Workspace `ws`'s board, from the work link log's fold and `trackers`, asked in order: the
/// first that holds a key fills its card.
pub fn of(ws: &Workspace, folded: &Fold, trackers: &[&dyn Tracker]) -> io::Result<Board> {
    let cards = list::of(ws, folded)?
        .into_iter()
        .map(|row| {
            // A todo the list read is a card with its title even when no `Todos` tracker was
            // given: the list already holds it.
            let item = trackers
                .iter()
                .find_map(|tracker| tracker.item(&row.key))
                .or_else(|| {
                    let title = row.todo.clone()?;
                    Some(WorkItem::new(row.key.clone(), Kind::Todo, title))
                });
            Card {
                item,
                key: row.key,
                chats: row.chats,
            }
        })
        .collect();
    Ok(Board { cards })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::log::{self, Op};
    use crate::work::tracker::Todos;
    use crate::workspaces::Plane;

    const DEVICE: &str = "01K6H0Z8Y3V1N3G4QK0A9T5B7C";
    const CHAT: &str = "01K6H10000AAAAAAAAAAAAAAAA";

    fn stamp(second: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 2)
            .unwrap()
            .and_hms_opt(8, 0, second)
            .unwrap()
    }

    #[test]
    fn a_chat_linked_todo_is_a_todo_card_with_its_chat_on_it() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = Plane::open(tmp.path()).workspace("alpha").unwrap();
        let picker = ws.add_todo("Port the picker", stamp(0)).unwrap();
        let stem = picker.file_stem().unwrap().to_string_lossy().into_owned();
        let todo = TrackerKey::todo("alpha", &stem).unwrap();
        log::append(
            tmp.path(),
            "alpha",
            DEVICE,
            stamp(1).and_utc(),
            &Op::link_chat(todo.clone(), CHAT),
        )
        .unwrap();

        let todos = Todos::of(&ws).unwrap();
        let board = of(&ws, &log::fold(tmp.path()), &[&todos]).unwrap();
        assert_eq!(board.cards.len(), 1, "{board:?}");
        let card = &board.cards[0];
        assert_eq!(card.key, todo);
        assert_eq!(card.chats, [CHAT]);
        let item = card.item.as_ref().expect("the todo tracker fills it");
        assert_eq!(
            (item.kind, item.title.as_str()),
            (Kind::Todo, "Port the picker")
        );
        assert_eq!(card.column(), Column::Open);
    }

    #[test]
    fn an_item_no_tracker_holds_is_still_a_card_and_stands_open() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = Plane::open(tmp.path()).workspace("alpha").unwrap();
        let issue = TrackerKey::parse("gitlab:gitlab.com/acme/sub/api#4").unwrap();
        log::append(
            tmp.path(),
            "alpha",
            DEVICE,
            stamp(1).and_utc(),
            &Op::link(issue.clone()),
        )
        .unwrap();

        let board = of(&ws, &log::fold(tmp.path()), &[]).unwrap();
        assert_eq!(
            board.cards,
            [Card {
                key: issue,
                item: None,
                chats: vec![]
            }]
        );
        assert_eq!(board.column(Column::Open).count(), 1);
    }

    #[test]
    fn a_closed_item_stands_in_the_closed_column_and_the_first_tracker_that_holds_it_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = Plane::open(tmp.path()).workspace("alpha").unwrap();
        let issue = TrackerKey::parse("github:github.com/acme/api#7").unwrap();
        log::append(
            tmp.path(),
            "alpha",
            DEVICE,
            stamp(1).and_utc(),
            &Op::link(issue.clone()),
        )
        .unwrap();
        let mut closed = WorkItem::new(issue.clone(), Kind::Issue, "Ship it");
        closed.state = State::Closed;
        let first = vec![closed];
        let second = vec![WorkItem::new(issue.clone(), Kind::Issue, "A stale copy")];

        let folded = log::fold(tmp.path());
        let board = of(&ws, &folded, &[&first, &second]).unwrap();
        assert_eq!(board.column(Column::Open).count(), 0);
        let done: Vec<&str> = board
            .column(Column::Closed)
            .filter_map(|card| card.item.as_ref())
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(done, ["Ship it"]);
        assert_eq!(
            of(&ws, &folded, &[&first, &second]).unwrap(),
            board,
            "a board rebuilt from the same sources is the same board"
        );
    }

    #[test]
    fn a_todo_is_a_card_with_its_title_even_with_no_todo_tracker_given() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = Plane::open(tmp.path()).workspace("alpha").unwrap();
        let picker = ws.add_todo("Port the picker", stamp(0)).unwrap();
        let stem = picker.file_stem().unwrap().to_string_lossy().into_owned();

        let board = of(&ws, &log::fold(tmp.path()), &[]).unwrap();
        let item = board.cards[0]
            .item
            .as_ref()
            .expect("the list's todo fills it");
        assert_eq!(item.key, TrackerKey::todo("alpha", &stem).unwrap());
        assert_eq!(
            (item.kind, item.title.as_str()),
            (Kind::Todo, "Port the picker")
        );
    }
}
