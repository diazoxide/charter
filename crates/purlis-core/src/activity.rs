//! A session's Activity: one timeline of what a chat and its tasks said to each other (#1495,
//! spec #1483, V100-44).
//!
//! A **session** here is any chat. Its timeline is every task it dispatched and everything
//! under them, the tasks of its tasks included, as one list in time order: each dispatch with
//! its brief, each follow-up, progress note, question and answer, and the report each task
//! ended with. A handoff is not a task (V100-69) and is on no timeline.
//!
//! # Where the lines come from (D-1495-1)
//!
//! **The dispatch records, and nothing else** ([`crate::dispatchrecord`]). A record already
//! held a dispatch's start, its brief, its end and its report. The messages between the two
//! chats it did not hold: each waits in the project only until the chat it is for reads it
//! ([`crate::dispatchtalk`]), so a timeline read afterwards finds none. So the app keeps each
//! one's text on the task's record as it takes it ([`crate::dispatchrecord::said`]), in the
//! write that already counted it. No file is added, and the record's rules are the
//! timeline's: the store a sandboxed chat can neither read nor write, the 30 days it is kept,
//! the caps, and the check on the way back to the screen ([`crate::dispatchrecord::sound`]).
//! A record that does not pass is counted ([`Timeline::refused`]) and none of it is listed.
//!
//! **The words are kept for 30 days after the task ended** (D-1495-12,
//! [`dispatchrecord::expire_talk`]): after that a message's line stays, with its time and its
//! kind, and says its words are no longer kept. **A record keeps the first messages**, and the
//! timeline has one line, in the task's place, for those it did not keep ([`Kind::Unkept`]).
//!
//! **Nothing of a chat's conversation is here.** A line is a message one chat sent another
//! through purlis, which is what the asking chat's turn was handed too.
//!
//! # What a line's text is
//!
//! A chat's words, as it sent them: **text, and never markup**. Nothing here reads it, and the
//! window draws it as characters. The one thing read out of a chat's words is which files a
//! report says it changed ([`paths_named`]), to mark a file two tasks touched (V100-68): a
//! mark for the person, which decides nothing.

use std::path::Path;

use crate::dispatchrecord::{self, Changed, ChatRef, Mode, Outcome, Record};
use crate::dispatchtalk;

/// What a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The asking chat dispatched the task: the line's text is the brief.
    Dispatched,
    /// From the asking chat to its running task.
    FollowUp,
    /// From a task to its asking chat: where it has got to.
    Note,
    /// From a task to its asking chat: it pauses until answered.
    Question,
    /// From the asking chat, on the question its task asked.
    Answer,
    /// The report the task ended with, its own or the app's in its place.
    Report,
    /// The person stopped the task before it reported ([`Outcome::Stopped`]).
    Stopped,
    /// Messages the record counted and kept no text of ([`Line::unkept`]): one line for all
    /// of them, where they would have stood. purlis's own line, and nobody's words.
    Unkept,
}

impl Kind {
    /// The word the window shows for it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Dispatched => "dispatched",
            Self::FollowUp => "follow-up",
            Self::Note => "note",
            Self::Question => "question",
            Self::Answer => "answer",
            Self::Report => "report",
            Self::Stopped => "stopped",
            Self::Unkept => "not listed",
        }
    }

    fn of(sent: dispatchtalk::Kind) -> Self {
        match sent {
            dispatchtalk::Kind::FollowUp => Self::FollowUp,
            dispatchtalk::Kind::Note => Self::Note,
            dispatchtalk::Kind::Question => Self::Question,
            dispatchtalk::Kind::Answer => Self::Answer,
        }
    }

    /// Whether the task said it, and not the chat that asked.
    fn said_by_the_task(self) -> bool {
        matches!(
            self,
            Self::Note | Self::Question | Self::Report | Self::Stopped | Self::Unkept
        )
    }
}

/// Why a record kept no text of some of its messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// They were sent before records kept any text: the record has none at all.
    Before,
    /// The record holds as many messages as one keeps ([`dispatchrecord::MOST_SAID`]).
    Count,
    /// The record holds as much text as one keeps ([`dispatchrecord::MOST_SAID_BYTES`]).
    Size,
}

impl Why {
    /// The word the window is handed.
    pub fn word(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::Count => "count",
            Self::Size => "size",
        }
    }
}

/// One line of a timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The dispatch it belongs to, by its record's id.
    pub dispatch: String,
    /// Its place among that dispatch's lines, from 0: with `dispatch`, what names the line.
    pub n: u32,
    /// When, UTC ([`crate::dispatch::stamp`]).
    pub at: String,
    pub kind: Kind,
    /// The chat that said it, as the record names it: the chat the line opens.
    pub from: ChatRef,
    /// The chat it was said to.
    pub to: ChatRef,
    /// What was said: the brief, the message, or the report's text. Empty for a message
    /// whose words are no longer kept ([`Line::expired`]), and for [`Kind::Unkept`].
    pub text: String,
    /// **The person said it, and not the chat it is `from`.** On the dispatch's own line: the
    /// person dispatched the task themselves, from the asking chat's tab (V100-70). On an
    /// answer: the person answered the task's question in the purlis window (#1496), in the
    /// asking chat's place. The words are theirs either way.
    pub by_person: bool,
    /// **purlis wrote this line, and not the task**: the ending it recorded in a chat's place
    /// (ended without a report, stopped by the person), and [`Kind::Unkept`].
    pub by_purlis: bool,
    /// An answer the person gave that the task was never handed: it ended first
    /// ([`dispatchrecord::Said::unread`]).
    pub unread: bool,
    /// A message whose words were kept and are not any more
    /// ([`dispatchrecord::expire_talk`]).
    pub expired: bool,
    /// A message whose text read like a credential, and was never kept
    /// ([`dispatchrecord::Said::left_out`], #1520).
    pub left_out: bool,
    /// For [`Kind::Unkept`]: how many messages the record counted after the last it kept,
    /// and why it kept no more.
    pub unkept: Option<(u32, Why)>,
    /// How the task ended, on the line that ends it.
    pub outcome: Option<Outcome>,
    /// The files that line's report says the task changed ([`paths_named`]).
    pub files: Vec<String>,
    /// The task's name, else its chat's.
    pub task: String,
    /// Where the task worked ([`place_of`]): two tasks with the same one worked in the same
    /// folder.
    pub place: String,
    /// How far under the session the task is: 1 for a task it dispatched itself.
    pub depth: u32,
}

/// A session's timeline.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Timeline {
    /// Oldest first.
    pub lines: Vec<Line>,
    /// How many of **this session's** tasks are not listed because purlis will not draw
    /// their records ([`dispatchrecord::sound`]): a record whose asking chat is the session or
    /// a chat on its timeline. Another session's is not counted here.
    pub refused: usize,
    /// How many of the tasks this session dispatched itself, the oldest, are not listed, with
    /// the tasks under them, because a timeline lists at most [`Bounds::tasks`] (#1520). The
    /// tasks under those are not counted here.
    pub unlisted: usize,
    /// How many of the project's dispatch records, the oldest, were not read because a
    /// timeline reads at most [`Bounds::records`] (#1520). Which session's they are is not
    /// known: they were not read.
    pub unread: usize,
}

/// **How much one timeline reads and lists** (#1520), so a session with many records, in a
/// project with more, is read in bounded time and draws a bounded list. What is past either
/// bound is counted ([`Timeline::unlisted`], [`Timeline::unread`]) and said, never silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// The newest records of the project read ([`dispatchrecord::newest`]).
    pub records: usize,
    /// The newest tasks the session dispatched itself that are listed, each with every task
    /// under it.
    pub tasks: usize,
}

impl Bounds {
    /// What the Activity tab reads: the newest 2,000 records, and 200 tasks.
    pub const TAB: Self = Self {
        records: 2_000,
        tasks: 200,
    };
}

/// **Where `record`'s task worked**, as one word to compare: its workspace and the folder it
/// started in. A task given a branch of its own started in that branch's folder, so it shares
/// a place with no other task; two tasks sent to the same folder share theirs.
pub fn place_of(record: &Record) -> String {
    format!(
        "{}\u{0}{}",
        record.place.workspace.as_deref().unwrap_or_default(),
        record.place.folder.as_deref().unwrap_or_default()
    )
}

/// Whether `report` is one purlis wrote in a chat's place: the person stopped the chat, or it
/// ended owing a report.
fn purlis_wrote(report: &dispatchrecord::Report) -> bool {
    report.outcome == Outcome::Stopped
        || report.text == dispatchrecord::ENDED_WITHOUT_A_REPORT
        || report.text == crate::handback::UNREPORTED
        || report.text == crate::handback::STOPPED
}

/// The lines of one task's `record`, oldest first, for a task `depth` under its session: the
/// dispatch, each message its record kept, one line for the messages it did not, and its
/// report once it has one.
pub fn lines_of(record: &Record, depth: u32) -> Vec<Line> {
    let task = record
        .task
        .clone()
        .unwrap_or_else(|| record.worker.chat.name.clone());
    let place = place_of(record);
    let line = |n: usize, at: &str, kind: Kind, text: &str| {
        let (from, to) = if kind.said_by_the_task() {
            (&record.worker.chat, &record.asker.chat)
        } else {
            (&record.asker.chat, &record.worker.chat)
        };
        Line {
            dispatch: record.id.clone(),
            n: u32::try_from(n).unwrap_or(u32::MAX),
            at: at.to_owned(),
            kind,
            from: from.clone(),
            to: to.clone(),
            text: text.to_owned(),
            by_person: false,
            by_purlis: false,
            unread: false,
            expired: false,
            left_out: false,
            unkept: None,
            outcome: None,
            files: Vec::new(),
            task: task.clone(),
            place: place.clone(),
            depth,
        }
    };
    let mut lines = vec![Line {
        by_person: record.asker.by_person,
        ..line(0, &record.started, Kind::Dispatched, &record.brief)
    }];
    for said in &record.talk {
        lines.push(Line {
            // The person's answer, which the record says is theirs (#1496).
            by_person: said.by == Some(dispatchrecord::By::Person),
            unread: said.unread,
            // A message is never taken empty but where it was left out: one with no words
            // had them, and they are gone.
            expired: said.text.is_empty() && !said.left_out,
            left_out: said.left_out,
            ..line(lines.len(), &said.at, Kind::of(said.kind), &said.text)
        });
    }
    // The messages after the last one kept, where they would have stood: before the report.
    let kept = u32::try_from(record.talk.len()).unwrap_or(u32::MAX);
    let unkept = record.messages.saturating_sub(kept);
    if unkept > 0 {
        let why = if record.talk.is_empty() {
            Why::Before
        } else if record.talk.len() >= dispatchrecord::MOST_SAID {
            Why::Count
        } else {
            Why::Size
        };
        let at = lines
            .last()
            .map_or(&record.started, |last| &last.at)
            .clone();
        lines.push(Line {
            by_purlis: true,
            unkept: Some((unkept, why)),
            ..line(lines.len(), &at, Kind::Unkept, "")
        });
    }
    if let (Some(report), Some(ended)) = (&record.report, &record.ended) {
        let kind = match report.outcome {
            Outcome::Stopped => Kind::Stopped,
            _ => Kind::Report,
        };
        lines.push(Line {
            by_purlis: purlis_wrote(report),
            outcome: Some(report.outcome),
            files: paths_named(&report.changed),
            ..line(lines.len(), ended, kind, &report.text)
        });
    }
    lines
}

/// The tasks under `session` among `records`, each with its depth: the tasks it dispatched,
/// then the tasks of those, and so on down. A chat is matched as a record names it
/// ([`dispatchrecord::same_chat`]), and each record is taken once, so records that name each
/// other in a ring end where they began.
fn under<'a>(records: &'a [Record], session: &ChatRef) -> Vec<(&'a Record, u32)> {
    under_tops(records, session)
        .into_iter()
        .map(|(record, depth, _)| (record, depth))
        .collect()
}

/// [`under`], each task with the id of the session's own task it is under (its own, for a
/// task the session dispatched itself).
fn under_tops<'a>(records: &'a [Record], session: &ChatRef) -> Vec<(&'a Record, u32, &'a str)> {
    let mut found: Vec<(&Record, u32, &str)> = Vec::new();
    let mut askers: Vec<(ChatRef, u32, Option<&str>)> = vec![(session.clone(), 0u32, None)];
    while let Some((asker, depth, top)) = askers.pop() {
        for record in records {
            let theirs = record.mode == Mode::Task
                && dispatchrecord::same_chat(&record.asker.chat, &asker)
                && !found.iter().any(|(taken, _, _)| taken.id == record.id);
            if theirs {
                let top = top.unwrap_or(record.id.as_str());
                found.push((record, depth + 1, top));
                askers.push((record.worker.chat.clone(), depth + 1, Some(top)));
            }
        }
    }
    found
}

/// The timeline of `session` among `records`, which are ones purlis draws.
pub fn of(records: &[Record], session: &ChatRef) -> Timeline {
    Timeline {
        lines: lines_in(under(records, session)),
        ..Timeline::default()
    }
}

/// The lines of `tasks`, each at its depth, in time order.
fn lines_in(tasks: Vec<(&Record, u32)>) -> Vec<Line> {
    let mut lines: Vec<(String, Line)> = Vec::new();
    for (record, depth) in tasks {
        // **A task's own lines never change places.** Each is sorted by its time, and never
        // by one earlier than the line before it: where the clock stepped back between a
        // question and its answer, the answer still stands after the question.
        let mut latest = String::new();
        for line in lines_of(record, depth) {
            if line.at > latest {
                latest.clone_from(&line.at);
            }
            lines.push((latest.clone(), line));
        }
    }
    // A stamp sorts as its time, and a record's id as the time it was minted: two lines of
    // one second stand in the order their dispatches started, and a dispatch's own in the
    // order it kept them.
    lines.sort_by(|(at, a), (bt, b)| (at, &a.dispatch, a.n).cmp(&(bt, &b.dispatch, b.n)));
    lines.into_iter().map(|(_, line)| line).collect()
}

/// The timeline of `session` in the project at `root`, read at `now`: its tasks and
/// everything under them, in time order, from the records purlis draws
/// ([`dispatchrecord::sound`]).
///
/// What a record that ended 30 days ago kept of its messages' words is taken out first
/// ([`dispatchrecord::expire_in`]), so a timeline never shows words past their time, however
/// long the project has been open. **The store is read once**, within [`Bounds::TAB`], and the
/// expiry works on that one read (#1520).
pub fn timeline(root: &Path, session: &ChatRef, now: chrono::DateTime<chrono::Utc>) -> Timeline {
    timeline_within(root, session, now, Bounds::TAB)
}

/// [`timeline`], reading and listing within `bounds`.
pub fn timeline_within(
    root: &Path,
    session: &ChatRef,
    now: chrono::DateTime<chrono::Utc>,
    bounds: Bounds,
) -> Timeline {
    let (mut read, unread) = dispatchrecord::newest(root, bounds.records, now);
    dispatchrecord::expire_in(root, &mut read, now);
    let (drawn, refused): (Vec<Record>, Vec<Record>) =
        read.into_iter().partition(dispatchrecord::sound);
    let tasks = under_tops(&drawn, session);
    // The chats on the timeline: the session, and every task under it, listed or not.
    let mut on_it = vec![session.clone()];
    on_it.extend(
        tasks
            .iter()
            .map(|(record, _, _)| record.worker.chat.clone()),
    );
    // The session's own newest tasks, each whole with the tasks under it, so a listed task's
    // parent is always listed: a record's id sorts as the time it was minted.
    let mut tops: Vec<&str> = tasks
        .iter()
        .filter(|(_, depth, _)| *depth == 1)
        .map(|(record, _, _)| record.id.as_str())
        .collect();
    tops.sort_by(|a, b| b.cmp(a));
    let unlisted = tops.len().saturating_sub(bounds.tasks);
    tops.truncate(bounds.tasks);
    let lines = lines_in(
        tasks
            .iter()
            .filter(|(_, _, top)| tops.contains(top))
            .map(|(record, depth, _)| (*record, *depth))
            .collect(),
    );
    let refused = refused
        .iter()
        .filter(|record| {
            record.mode == Mode::Task
                && on_it
                    .iter()
                    .any(|chat| dispatchrecord::same_chat(&record.asker.chat, chat))
        })
        .count();
    Timeline {
        lines,
        refused,
        unlisted,
        unread,
    }
}

/// **The files a report says its task changed**: the ones it lists (`changed.files`), then the
/// ones its own words name (`changed.said`), each once, a leading `./` dropped.
///
/// A report's words are prose, so this takes only what reads as a file: a word whose last part
/// has an extension that begins with a letter (`src/app.rs`, `values.yaml`), with the marks
/// around it dropped. A branch (`fix/rollout`), a commit, a version (`v1.2`) and an address
/// are not files. **It is a reading of a chat's claim, for a mark the person sees**, and
/// nothing is decided by it: a file named another way is not found, and a word that only
/// looks like a file is.
pub fn paths_named(changed: &Changed) -> Vec<String> {
    let said = changed.said.as_deref().unwrap_or_default();
    let mut named: Vec<String> = Vec::new();
    let listed = changed.files.iter().map(String::as_str);
    let read = said.split_whitespace().filter_map(a_file);
    for path in listed.chain(read) {
        let path = path.strip_prefix("./").unwrap_or(path);
        if !path.is_empty()
            && named.len() < dispatchrecord::MOST_LISTED
            && !named.iter().any(|one| one == path)
        {
            named.push(path.to_owned());
        }
    }
    named
}

/// `word` as the file it names, where it reads as one ([`paths_named`]).
fn a_file(word: &str) -> Option<&str> {
    let around: &[char] = &['`', '\'', '"', '(', ')', '[', ']', '<', '>', ',', ';', ':'];
    let word = word.trim_matches(around).trim_end_matches('.');
    let word = word.trim_matches(around);
    if word.contains("://") {
        return None;
    }
    let last = word.rsplit('/').next()?;
    let (stem, extension) = last.rsplit_once('.')?;
    let lettered = extension.starts_with(|c: char| c.is_ascii_alphabetic())
        && extension.len() <= 10
        && extension.chars().all(|c| c.is_ascii_alphanumeric());
    // `e.g` and `i.e` are not files; `a.rs` in a folder is.
    let named = stem.chars().count() >= 2 || (word.contains('/') && !stem.is_empty());
    (lettered && named).then_some(word)
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
