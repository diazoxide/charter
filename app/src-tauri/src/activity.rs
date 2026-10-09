//! The app's half of a session's Activity (#1495, V100-44): handing the window one chat's
//! timeline, and telling it each line as the app records it.
//!
//! `purlis_core::activity` is the timeline and argues where its lines come from. This module
//! adds the two things only the app has: **which chats are open now**, so a line can open the
//! chat it came from, and **the window**, which is told each new line as it is written
//! ([`EVENT`]) so that an open Activity tab follows the work without reading the records again.
//!
//! A line is told from the three places the app already writes a dispatch's record
//! (`crate::dispatches`): where it opens one ([`dispatched`]), where it takes a message on one
//! ([`said`], [`unkept`]) and where it closes one ([`ended`]). Each tells what the record now
//! says, never anything a chat sent, and to every Activity tab of the project: a tab keeps the
//! lines that are its session's.
//!
//! **A line carries a chat's words, so it goes to the window that holds the project and to no
//! other** ([`to_its_window`]): where no window holds it yet, nothing is sent, and the tab that
//! opens later reads the records.

use std::sync::Arc;

use purlis_core::activity::{self, Line};
use purlis_core::dispatchrecord::{self, ChatRef, Mode, Record};
use purlis_core::dispatchtalk;

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
    /// `dispatched`, `follow-up`, `note`, `question`, `answer`, `report`, `stopped`, or
    /// `not listed` for the one line that stands for messages the record kept no text of.
    pub kind: String,
    /// The chat that said it, by the name the person saw. **As the window draws it**
    /// (`dispatchtalk::chat_shown`): a name that reads like one of the app's own marks for who
    /// spoke has "(a chat)" after it, here and in `to` and `task`, so no name passes for one.
    pub from: String,
    /// Which chat that is: its id, or `#<number>` for one given none.
    pub from_key: String,
    /// That chat's session, while it is still open: what the line opens.
    pub from_session: Option<u32>,
    /// The chat it was said to, the same two ways.
    pub to: String,
    pub to_key: String,
    /// What was said, **as text**: a chat's own words, never drawn as markup. Empty where
    /// `expired`, and for a `not listed` line.
    pub text: String,
    /// The person said it, and not the chat it is `from`: on a dispatch's own line, they
    /// dispatched the task themselves from that chat's tab; on an answer, they answered the
    /// task's question in the window (#1496). The words are theirs.
    pub by_person: bool,
    /// **On a question its task is paused on now, that question's number** (#1496): the
    /// person may answer it, and the number is what their answer is for. `null` on every other
    /// line. Said by the app from what it holds open as the line is read or told, and never
    /// by the record.
    pub asks: Option<u32>,
    /// **On an answer told as it is recorded, the number of the question it answered**, whoever
    /// gave it; and on a `not listed` line told because an answer's words were not kept. It
    /// closes that question in an open tab, by number and not by where the line stands. `null`
    /// on a line that was read: there `asks` already says which question is open.
    pub answers: Option<u32>,
    /// An answer the person gave that the task was never handed: it ended first.
    pub unread: bool,
    /// purlis wrote the line, and not the task: an ending it recorded in a chat's place, and
    /// a `not listed` line.
    pub by_purlis: bool,
    /// A message whose words were kept for 30 days after its task ended, and are gone.
    pub expired: bool,
    /// A message whose text read like a credential, so purlis never kept it (#1520). Its
    /// `text` is empty.
    pub left_out: bool,
    /// On a `not listed` line: how many messages the record counted after the last it kept.
    pub unkept: Option<u32>,
    /// And why it kept no more: `before` (they were sent before records kept any text),
    /// `count` (a record keeps so many messages) or `size` (so much text).
    pub unkept_why: Option<String>,
    /// How the task ended, in the report's word, on the line that ends it.
    pub outcome: Option<String>,
    /// The files that line's report says the task changed, as far as its words name them.
    pub files: Vec<String>,
    /// The task's name, else its chat's.
    pub task: String,
    /// Where the task worked, as one word to compare: two tasks with the same one worked in
    /// the same folder.
    pub place: String,
    /// How far under the session the task is, 1 for a task it dispatched itself, **in a
    /// timeline that was read**. 0 on a line told as it lands: which timeline it is on, and
    /// how deep, is the tab's to say there.
    pub depth: u32,
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
    /// How many of this chat's tasks, and of the tasks under them, are not listed because
    /// purlis will not draw their records.
    pub undrawn: u32,
    /// How many of the tasks this chat dispatched itself, the oldest, are not listed, with the
    /// tasks under them: a timeline lists the newest `most_listed` (#1520).
    pub unlisted: u32,
    /// How many of the project's dispatch records, the oldest, were not read: a timeline reads
    /// the newest `most_read` (#1520). Whose they are is not known.
    pub unread: u32,
    /// The bounds that were read within (`activity::Bounds::TAB`), for the tab to say.
    pub most_listed: u32,
    pub most_read: u32,
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

/// The session `chat`, a chat as a record names it, has now, where it is open.
pub(crate) fn session_of(chat: &ChatRef, open: &[OpenChat]) -> Option<u32> {
    // By the chat's id, which a restart keeps; by its number only for a record with no id.
    open.iter()
        .find(|one| dispatchrecord::named(chat, one.id.as_deref(), Some(one.session)))
        .map(|one| one.session)
}

/// `line` as the window draws it, given the chats open now. `depth` is the line's own in a
/// timeline that was read, and 0 for a line told as it lands.
pub(crate) fn drawn(line: &Line, open: &[OpenChat], depth: u32) -> ActivityLine {
    let from_session = session_of(&line.from, open);
    ActivityLine {
        dispatch: line.dispatch.clone(),
        n: line.n,
        at: line.at.clone(),
        kind: line.kind.word().to_owned(),
        from: dispatchtalk::chat_shown(&line.from.name),
        from_key: key(&line.from),
        from_session,
        to: dispatchtalk::chat_shown(&line.to.name),
        to_key: key(&line.to),
        text: line.text.clone(),
        by_person: line.by_person,
        asks: None,
        answers: None,
        unread: line.unread,
        by_purlis: line.by_purlis,
        expired: line.expired,
        left_out: line.left_out,
        unkept: line.unkept.map(|(count, _)| count),
        unkept_why: line.unkept.map(|(_, why)| why.word().to_owned()),
        outcome: line.outcome.map(|outcome| outcome.word().to_owned()),
        files: line.files.clone(),
        task: dispatchtalk::chat_shown(&line.task),
        place: line.place.clone(),
        depth,
    }
}

/// The question chat `task` is paused on, as the app holds it: its number and its words.
fn open_question(held: &Held, task: u32) -> Option<(u32, String)> {
    held.tasks()
        .ledger()
        .talk
        .open(task)
        .map(|(number, text)| (number, text.to_owned()))
}

/// The timeline of chat `session`: its tasks and everything under them, oldest first. `None`
/// for a chat this app does not have open: nothing to read again, and the tab says so.
pub(crate) fn read(held: &Held, session: u32) -> Option<Activity> {
    let chat = crate::dispatches::chat_ref(held, session)?;
    let open = crate::dispatches::open_chats(held);
    let bounds = activity::Bounds::TAB;
    let found = activity::timeline_within(held.root(), &chat, chrono::Utc::now(), bounds);
    let mut lines: Vec<ActivityLine> = found
        .lines
        .iter()
        .map(|line| drawn(line, &open, line.depth))
        .collect();
    still_asked(&mut lines, |task| open_question(held, task));
    Some(Activity {
        key: key(&chat),
        name: chat.name,
        lines,
        undrawn: u32::try_from(found.refused).unwrap_or(u32::MAX),
        unlisted: u32::try_from(found.unlisted).unwrap_or(u32::MAX),
        unread: u32::try_from(found.unread).unwrap_or(u32::MAX),
        most_listed: u32::try_from(bounds.tasks).unwrap_or(u32::MAX),
        most_read: u32::try_from(bounds.records).unwrap_or(u32::MAX),
    })
}

/// Marks, among `lines`, **each question its task is paused on now, with its number**
/// ([`ActivityLine::asks`]): the last question of a dispatch, from a chat that is open, whose
/// words are the question `asks` says that chat has open. An earlier question of the same task
/// was answered, and a question whose words are not kept cannot be shown to be answered.
fn still_asked(lines: &mut [ActivityLine], asks: impl Fn(u32) -> Option<(u32, String)>) {
    let mut seen: Vec<String> = Vec::new();
    for line in lines.iter_mut().rev() {
        if line.kind != activity::Kind::Question.word() || seen.contains(&line.dispatch) {
            continue;
        }
        seen.push(line.dispatch.clone());
        line.asks = line
            .from_session
            // A question whose words are gone, or were never kept, is not one to answer here.
            .filter(|_| !line.expired && !line.left_out)
            .and_then(&asks)
            .filter(|(_, open)| *open == line.text)
            .map(|(number, _)| number);
    }
}

/// One chat's Activity (#1495): what it and its tasks said to each other, and the tasks of
/// its tasks, as one timeline, oldest first. Read-only. `null` for a chat that is not open.
/// On a blocking thread, as it reads every dispatch record.
#[tauri::command]
#[specta::specta]
pub(crate) async fn activity(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Option<Activity>, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading a chat's activity", move || {
        Ok(read(&held, session))
    })
    .await
}

/// **The session the chat a line names has now** (#1495): `key` is the line's `from_key`, a
/// chat's id or `#<number>`. `null` where that chat is not open.
///
/// Asked when a line's chat is pressed, so the press reaches the chat under the number it has
/// then: a chat that was restarted since the timeline was read has another, and one that was
/// closed has none.
#[tauri::command]
#[specta::specta]
pub(crate) fn activity_chat(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: PlaneId,
    key: String,
) -> Result<Option<u32>, String> {
    let held = planes.held(&plane)?;
    Ok(chat_now(&key, &crate::dispatches::open_chats(&held)))
}

/// [`activity_chat`], over the chats open now.
fn chat_now(key: &str, open: &[OpenChat]) -> Option<u32> {
    match key.strip_prefix('#') {
        // A chat a record named by number alone: the chat with that number and no id.
        Some(number) => {
            let number: u32 = number.parse().ok()?;
            open.iter()
                .find(|one| one.session == number && one.id.is_none())
                .map(|one| one.session)
        }
        None => open
            .iter()
            .find(|one| one.id.as_deref() == Some(key))
            .map(|one| one.session),
    }
}

/// Sends `heard` to the window that holds its project, **and to no other**: where no window
/// holds it yet, nothing is sent. A line carries what a chat said, and a window that holds
/// another project has no use for it; a tab that opens later reads the records.
pub fn to_its_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>, heard: &ActivityHeard) {
    use tauri::{Emitter, Manager};
    let holder = app
        .try_state::<crate::planes::Showing>()
        .and_then(|showing| showing.holder(&heard.plane));
    if let Some(label) = holder {
        let _ = app.emit_to(label.as_str(), EVENT, heard);
    }
}

/// Tells the window `line` of `record`, where the record is one a timeline lists: a task's,
/// holding only text purlis draws.
fn tell(held: &Held, record: &Record, line: Option<&Line>, answers: Option<u32>) {
    let Some(line) = line else {
        return;
    };
    if record.mode != Mode::Task || !dispatchrecord::sound(record) {
        return;
    }
    let mut told = [drawn(line, &crate::dispatches::open_chats(held), 0)];
    // A question told as it is asked is one its task is paused on: the person may answer it.
    still_asked(&mut told, |task| open_question(held, task));
    let [mut told] = told;
    told.answers = answers;
    held.tell_activity(told);
}

/// The app opened `record`: its first line, the dispatch and its brief.
pub(crate) fn dispatched(held: &Held, record: &Record) {
    tell(held, record, activity::lines_of(record, 1).first(), None);
}

/// The app kept a message on `record`, which is the record as it now stands: its newest line.
/// `answers` is the number of the question it answered, where it is an answer (#1496).
pub(crate) fn said(held: &Held, record: &Record, answers: Option<u32>) {
    tell(held, record, activity::lines_of(record, 1).last(), answers);
}

/// The app counted a message on `record` and kept no text of it: the one line that says how
/// many it did not keep, as it now stands. No words of the message are told. `answers` is the
/// number of the question that message answered, where it was an answer: the question is
/// closed in an open tab though no answer's line says so.
pub(crate) fn unkept(held: &Held, record: &Record, answers: Option<u32>) {
    let lines = activity::lines_of(record, 1);
    tell(
        held,
        record,
        lines.iter().find(|line| line.unkept.is_some()),
        answers,
    );
}

/// Dispatch `id` ended before its task was handed the person's answer (#1496), and its record
/// now says so: that answer's line is told again as it stands.
pub(crate) fn answer_unread(held: &Held, id: &str) {
    let Some(record) = dispatchrecord::read(held.root(), id) else {
        return;
    };
    let lines = activity::lines_of(&record, 1);
    tell(
        held,
        &record,
        lines.iter().rev().find(|line| line.unread),
        None,
    );
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
        None,
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
            by_person: false,
            by_purlis: false,
            unread: false,
            expired: false,
            left_out: false,
            unkept: None,
            outcome: Some(Outcome::Done),
            files: vec!["src/app.rs".to_owned()],
            task: "talk".to_owned(),
            place: "alpha\u{0}workspaces/alpha".to_owned(),
            depth: 2,
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

        let line = drawn(&a_report(), &open, 2);

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

    /// A question `talk` (chat 7, dispatch `dispatch`) asked, as line `n`.
    fn a_question(dispatch: &str, n: u32, text: &str) -> ActivityLine {
        let line = Line {
            dispatch: dispatch.to_owned(),
            n,
            kind: Kind::Question,
            text: text.to_owned(),
            outcome: None,
            files: Vec::new(),
            ..a_report()
        };
        drawn(
            &line,
            &[OpenChat {
                session: 7,
                id: Some("worker".to_owned()),
            }],
            1,
        )
    }

    #[test]
    fn only_the_question_a_task_is_paused_on_now_is_one_the_person_may_answer() {
        let asked = |task: u32| (task == 7).then(|| (5, "Which region?".to_owned()));
        let mut lines = vec![
            // Answered long ago, by its words and by its place.
            a_question("A", 1, "Which queue?"),
            a_question("A", 3, "Which region?"),
            // Another task's, which the app holds no question for.
            ActivityLine {
                from_session: Some(8),
                ..a_question("B", 1, "Which region?")
            },
            // A report is never a question, whatever it says.
            ActivityLine {
                kind: "report".to_owned(),
                ..a_question("C", 1, "Which region?")
            },
        ];

        still_asked(&mut lines, asked);

        // The open one carries its number: what the person's answer is for.
        let asks: Vec<Option<u32>> = lines.iter().map(|line| line.asks).collect();
        assert_eq!(asks, [None, Some(5), None, None]);
    }

    #[test]
    fn a_question_is_not_one_to_answer_once_its_chat_is_closed_or_its_words_are_gone() {
        let asked = |_: u32| Some((5, "Which region?".to_owned()));
        let mut lines = vec![
            ActivityLine {
                from_session: None,
                ..a_question("A", 1, "Which region?")
            },
            ActivityLine {
                expired: true,
                text: String::new(),
                ..a_question("B", 1, "")
            },
            // The same words as the open question, in an earlier line of the task that has a
            // later question: the later one is the open one.
            a_question("C", 1, "Which region?"),
            a_question("C", 3, "Which zone?"),
        ];

        still_asked(&mut lines, asked);

        assert!(lines.iter().all(|line| line.asks.is_none()), "{lines:?}");
    }

    #[test]
    fn a_question_whose_text_was_left_out_is_not_one_to_answer_here() {
        // #1520: its words read like a credential and were never kept, so the person would
        // answer a question they cannot read. Even where the open question's words are as
        // empty as its line's.
        let asked = |_: u32| Some((5, String::new()));
        let mut lines = vec![ActivityLine {
            left_out: true,
            text: String::new(),
            ..a_question("A", 1, "")
        }];

        still_asked(&mut lines, asked);

        assert_eq!(lines[0].asks, None);
    }

    #[test]
    fn a_chat_named_like_the_app_s_marks_is_drawn_as_a_chat_wherever_its_name_stands() {
        // A task's name is chosen by the chat that dispatched it.
        let line = Line {
            from: chat(7, Some("worker"), "you"),
            to: chat(3, None, "purlis, for talk"),
            task: "The person".to_owned(),
            ..a_report()
        };

        let drawn = drawn(&line, &[], 1);

        assert_eq!(drawn.from, "you (a chat)");
        assert_eq!(drawn.to, "purlis, for talk (a chat)");
        assert_eq!(drawn.task, "The person (a chat)");
        // And a name that only begins like one is left as it is.
        let plain = super::drawn(
            &Line {
                from: chat(7, Some("worker"), "youth survey"),
                ..a_report()
            },
            &[],
            1,
        );
        assert_eq!(plain.from, "youth survey");
    }

    #[test]
    fn a_line_whose_chat_is_closed_opens_nothing() {
        assert_eq!(drawn(&a_report(), &[], 0).from_session, None);
    }

    #[test]
    fn a_line_carries_its_depth_where_it_was_read_and_what_it_does_not_hold() {
        use purlis_core::activity::Why;
        let gap = Line {
            kind: Kind::Unkept,
            text: String::new(),
            by_purlis: true,
            unkept: Some((3, Why::Size)),
            outcome: None,
            files: Vec::new(),
            ..a_report()
        };

        let line = drawn(&gap, &[], 2);

        assert_eq!(line.kind, "not listed");
        assert_eq!(
            (line.unkept, line.unkept_why.as_deref()),
            (Some(3), Some("size"))
        );
        assert!(line.by_purlis);
        assert_eq!(line.depth, 2);
        assert_eq!(line.place, "alpha\u{0}workspaces/alpha");
    }

    /// The app on Tauri's mock runtime, with a registry of windows and the two named.
    fn two_windows() -> (
        tauri::App<tauri::test::MockRuntime>,
        [tauri::WebviewWindow<tauri::test::MockRuntime>; 2],
    ) {
        use tauri::Manager;
        let app = tauri::test::mock_builder()
            .build(tauri_context!(test = true))
            .expect("the app builds");
        app.manage(crate::planes::Showing::default());
        let window = |label: &str| {
            tauri::WebviewWindowBuilder::new(&app, label, tauri::WebviewUrl::default())
                .build()
                .expect("a window")
        };
        let windows = [window("main"), window("split-1")];
        (app, windows)
    }

    fn plane(root: &str) -> PlaneId {
        serde_json::from_value(serde_json::json!(root)).expect("a plane id")
    }

    fn heard(plane: PlaneId) -> ActivityHeard {
        ActivityHeard {
            plane,
            line: drawn(&a_report(), &[], 0),
        }
    }

    /// How many `EVENT`s each of `windows` hears, and the app as a whole.
    fn counting(
        app: &tauri::App<tauri::test::MockRuntime>,
        windows: &[tauri::WebviewWindow<tauri::test::MockRuntime>; 2],
    ) -> [Arc<std::sync::atomic::AtomicUsize>; 3] {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tauri::Listener;
        let counts: [Arc<AtomicUsize>; 3] = Default::default();
        for (window, count) in windows.iter().zip(counts.iter()) {
            let count = count.clone();
            window.listen(EVENT, move |_| {
                count.fetch_add(1, Ordering::SeqCst);
            });
        }
        let any = counts[2].clone();
        app.listen_any(EVENT, move |_| {
            any.fetch_add(1, Ordering::SeqCst);
        });
        counts
    }

    fn read(counts: &[Arc<std::sync::atomic::AtomicUsize>; 3]) -> [usize; 3] {
        counts
            .each_ref()
            .map(|count| count.load(std::sync::atomic::Ordering::SeqCst))
    }

    #[test]
    fn a_line_goes_only_to_the_window_holding_its_project_and_nowhere_when_none_does() {
        // #1520: what a chat said is told to the one window that holds its project, through
        // the real registry of windows.
        use tauri::Manager;
        let (app, windows) = two_windows();
        let counts = counting(&app, &windows);
        let ours = plane("/projects/ours");
        app.state::<crate::planes::Showing>().in_window(
            "split-1",
            crate::planes::Holding {
                planes: vec![plane("/projects/other")],
                active: Some(0),
            },
        );

        // No window holds it yet: nothing is sent, to any window.
        to_its_window(app.handle(), &heard(ours.clone()));
        assert_eq!(read(&counts), [0, 0, 0]);

        app.state::<crate::planes::Showing>().in_window(
            "main",
            crate::planes::Holding {
                planes: vec![ours.clone()],
                active: Some(0),
            },
        );
        to_its_window(app.handle(), &heard(ours));

        // The window holding it, and no other: the window holding another project hears none.
        assert_eq!(read(&counts), [1, 0, 1]);
    }

    #[test]
    fn a_line_s_chat_is_found_under_the_number_it_has_now() {
        // Restart chat gave the task's chat another number; its id is the same.
        let open = [
            OpenChat {
                session: 12,
                id: Some("worker".to_owned()),
            },
            OpenChat {
                session: 3,
                id: None,
            },
            OpenChat {
                session: 4,
                id: Some("another".to_owned()),
            },
        ];

        assert_eq!(chat_now("worker", &open), Some(12));
        // A chat a record named by number alone is that number's chat, while it has no id.
        assert_eq!(chat_now("#3", &open), Some(3));
        assert_eq!(
            chat_now("#4", &open),
            None,
            "chat 4 has an id, and is another chat"
        );
        assert_eq!(chat_now("closed", &open), None);
        assert_eq!(chat_now("#x", &open), None);
    }
}
