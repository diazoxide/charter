//! The app's half of a session's Activity (#1495, V100-44): handing the window one chat's
//! timeline, and telling it each line as the app records it.
//!
//! `purlis_core::activity` is the timeline and argues where its lines come from. This module
//! adds the two things only the app has: **which chats are open now**, so a line can open the
//! chat it came from, and **the window**, which is told each new line as it is written
//! ([`EVENT`]) so that an open Activity tab follows the work without reading the records again.
//!
//! A line is told from the three places the app already writes a dispatch's record
//! (`crate::dispatches`): where it opens one ([`dispatched`]), where it keeps a message on one
//! ([`said`]) and where it closes one ([`ended`]). Each tells what the record now says, never
//! anything a chat sent, and to every Activity tab of the project: a tab keeps the lines that
//! are its session's.

use std::sync::Arc;

use purlis_core::activity::{self, Line};
use purlis_core::dispatchrecord::{self, ChatRef, Mode, Record};

use crate::dispatches::OpenChat;
use crate::planes::{Held, PlaneId};

/// The event a window hears for each line: an [`ActivityHeard`].
pub const EVENT: &str = "activity-line";

/// One line of a timeline, as the Activity tab draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ActivityLine {
    /// The dispatch it belongs to, by its record's id.
    pub dispatch: String,
    /// Its place among that dispatch's lines, from 0. With `dispatch`, what names the line: a
    /// line heard twice is drawn once.
    pub n: u32,
    /// When, as the record keeps it (UTC, RFC 3339).
    pub at: String,
    /// `dispatched`, `follow-up`, `note`, `question`, `answer`, `report` or `stopped`.
    pub kind: String,
    /// The chat that said it, by the name the person saw.
    pub from: String,
    /// Which chat that is: its id, or `#<number>` for one given none.
    pub from_key: String,
    /// That chat's session, while it is still open: what the line opens.
    pub from_session: Option<u32>,
    /// The chat it was said to, the same two ways.
    pub to: String,
    pub to_key: String,
    /// What was said, **as text**: a chat's own words, never drawn as markup.
    pub text: String,
    /// How the task ended, in the report's word, on the line that ends it.
    pub outcome: Option<String>,
    /// The files that line's report says the task changed, as far as its words name them.
    pub files: Vec<String>,
    /// The task's name, else its chat's.
    pub task: String,
}

/// What the Activity tab of one chat is handed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Activity {
    /// The chat, by the name the person sees it under.
    pub name: String,
    /// Which chat it is ([`ActivityLine::from_key`]): what a line heard later is matched to.
    pub key: String,
    /// Oldest first.
    pub lines: Vec<ActivityLine>,
    /// How many messages its tasks' records counted and did not keep the text of.
    pub unkept: u32,
    /// How many records in the store purlis will not draw.
    pub undrawn: u32,
}

/// A line the app has just recorded, and the project it is in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ActivityHeard {
    pub plane: PlaneId,
    pub line: ActivityLine,
}

/// Told each line of every plane's dispatches as it is recorded.
pub type Teller = Arc<dyn Fn(ActivityHeard) + Send + Sync + 'static>;

/// Which chat `chat` is, as a window matches one: its id, or `#<number>` for one given none
/// (`DispatchRow::asker_key`'s rule).
fn key(chat: &ChatRef) -> String {
    chat.id.clone().unwrap_or_else(|| format!("#{}", chat.chat))
}

/// `line` as the window draws it, given the chats open now.
pub(crate) fn drawn(line: &Line, open: &[OpenChat]) -> ActivityLine {
    // By the chat's id, which a restart keeps; by its number only for a record with no id.
    let from_session = open
        .iter()
        .find(|chat| dispatchrecord::named(&line.from, chat.id.as_deref(), Some(chat.session)))
        .map(|chat| chat.session);
    ActivityLine {
        dispatch: line.dispatch.clone(),
        n: line.n,
        at: line.at.clone(),
        kind: line.kind.word().to_owned(),
        from: line.from.name.clone(),
        from_key: key(&line.from),
        from_session,
        to: line.to.name.clone(),
        to_key: key(&line.to),
        text: line.text.clone(),
        outcome: line.outcome.map(|outcome| outcome.word().to_owned()),
        files: line.files.clone(),
        task: line.task.clone(),
    }
}

/// The timeline of chat `session`: its tasks and everything under them, oldest first.
pub(crate) fn read(held: &Held, session: u32) -> Result<Activity, String> {
    let chat = crate::dispatches::chat_ref(held, session)
        .ok_or_else(|| format!("chat {session} is not one this app has open"))?;
    let open = crate::dispatches::open_chats(held);
    let found = activity::timeline(held.root(), &chat);
    Ok(Activity {
        key: key(&chat),
        name: chat.name,
        lines: found.lines.iter().map(|line| drawn(line, &open)).collect(),
        unkept: found.unkept,
        undrawn: u32::try_from(found.refused).unwrap_or(u32::MAX),
    })
}

/// One chat's Activity (#1495): what it and its tasks said to each other, and the tasks of
/// its tasks, as one timeline, oldest first. Read-only. On a blocking thread, as it reads
/// every dispatch record.
#[tauri::command]
#[specta::specta]
pub(crate) async fn activity(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Activity, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading a chat's activity", move || read(&held, session)).await
}

/// Tells the window `line` of `record`, where the record is one a timeline lists: a task's,
/// holding only text purlis draws.
fn tell(held: &Held, record: &Record, line: Option<&Line>) {
    let Some(line) = line else {
        return;
    };
    if record.mode != Mode::Task || !dispatchrecord::sound(record) {
        return;
    }
    held.tell_activity(drawn(line, &crate::dispatches::open_chats(held)));
}

/// The app opened `record`: its first line, the dispatch and its brief.
pub(crate) fn dispatched(held: &Held, record: &Record) {
    tell(held, record, activity::lines_of(record, 1).first());
}

/// The app kept a message on `record`, which is the record as it now stands: its newest line.
pub(crate) fn said(held: &Held, record: &Record) {
    tell(held, record, activity::lines_of(record, 1).last());
}

/// The app closed dispatch `id`: the line of the report it ended with, where it ended with
/// one. Read back from the record, so the line says what the record says.
pub(crate) fn ended(held: &Held, id: &str) {
    let Some(record) = dispatchrecord::read(held.root(), id) else {
        return;
    };
    let lines = activity::lines_of(&record, 1);
    tell(
        held,
        &record,
        lines.last().filter(|line| line.outcome.is_some()),
    );
}

#[cfg(test)]
mod tests {
    use purlis_core::activity::Kind;
    use purlis_core::dispatchrecord::Outcome;

    use super::*;

    fn chat(number: u32, id: Option<&str>, name: &str) -> ChatRef {
        ChatRef {
            chat: number,
            id: id.map(str::to_owned),
            name: name.to_owned(),
            persona: None,
        }
    }

    fn a_report() -> Line {
        Line {
            dispatch: "01K6Z3V9QJ8M4T2W7XB5RC0DEF".to_owned(),
            n: 4,
            at: "2026-10-08T09:05:00+00:00".to_owned(),
            kind: Kind::Report,
            from: chat(7, Some("worker"), "talk"),
            to: chat(3, None, "steward 3"),
            text: "<b>Both</b> are healthy.".to_owned(),
            outcome: Some(Outcome::Done),
            files: vec!["src/app.rs".to_owned()],
            task: "talk".to_owned(),
            depth: 1,
        }
    }

    #[test]
    fn a_line_is_drawn_with_its_words_as_they_are_and_the_chat_it_opens() {
        // The task's chat was started again under another number: it is still that chat.
        let open = [
            OpenChat {
                session: 7,
                id: Some("another".to_owned()),
            },
            OpenChat {
                session: 12,
                id: Some("worker".to_owned()),
            },
        ];

        let line = drawn(&a_report(), &open);

        assert_eq!(line.kind, "report");
        assert_eq!(line.outcome.as_deref(), Some("done"));
        assert_eq!(line.text, "<b>Both</b> are healthy.");
        assert_eq!(
            (line.from.as_str(), line.from_key.as_str()),
            ("talk", "worker")
        );
        assert_eq!(line.from_session, Some(12));
        assert_eq!(
            (line.to.as_str(), line.to_key.as_str()),
            ("steward 3", "#3")
        );
        assert_eq!(line.files, ["src/app.rs"]);
        assert_eq!(
            (line.dispatch.as_str(), line.n),
            ("01K6Z3V9QJ8M4T2W7XB5RC0DEF", 4)
        );
    }

    #[test]
    fn a_line_whose_chat_is_closed_opens_nothing() {
        assert_eq!(drawn(&a_report(), &[]).from_session, None);
    }
}
