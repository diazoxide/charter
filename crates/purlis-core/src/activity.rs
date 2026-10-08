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
            Self::Note | Self::Question | Self::Report | Self::Stopped
        )
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
    /// What was said: the brief, the message, or the report's text.
    pub text: String,
    /// How the task ended, on the line that ends it.
    pub outcome: Option<Outcome>,
    /// The files that line's report says the task changed ([`paths_named`]).
    pub files: Vec<String>,
    /// The task's name, else its chat's.
    pub task: String,
    /// How far under the session the task is: 1 for a task it dispatched itself.
    pub depth: u32,
}

/// A session's timeline.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Timeline {
    /// Oldest first.
    pub lines: Vec<Line>,
    /// How many messages its tasks' records counted and did not keep the text of: sent before
    /// records kept any, or past what one record keeps.
    pub unkept: u32,
    /// How many records in the store purlis will not draw ([`dispatchrecord::sound`]). Of any
    /// session's: a record that is not read is not asked whose it is.
    pub refused: usize,
}

/// The lines of one task's `record`, oldest first, for a task `depth` under its session: the
/// dispatch, each message its record kept, and its report once it has one.
pub fn lines_of(record: &Record, depth: u32) -> Vec<Line> {
    let task = record
        .task
        .clone()
        .unwrap_or_else(|| record.worker.chat.name.clone());
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
            outcome: None,
            files: Vec::new(),
            task: task.clone(),
            depth,
        }
    };
    let mut lines = vec![line(0, &record.started, Kind::Dispatched, &record.brief)];
    for said in &record.talk {
        lines.push(line(lines.len(), &said.at, Kind::of(said.kind), &said.text));
    }
    if let (Some(report), Some(ended)) = (&record.report, &record.ended) {
        let kind = match report.outcome {
            Outcome::Stopped => Kind::Stopped,
            _ => Kind::Report,
        };
        lines.push(Line {
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
    let mut found: Vec<(&Record, u32)> = Vec::new();
    let mut askers = vec![(session.clone(), 0u32)];
    while let Some((asker, depth)) = askers.pop() {
        for record in records {
            let theirs = record.mode == Mode::Task
                && dispatchrecord::same_chat(&record.asker.chat, &asker)
                && !found.iter().any(|(taken, _)| taken.id == record.id);
            if theirs {
                found.push((record, depth + 1));
                askers.push((record.worker.chat.clone(), depth + 1));
            }
        }
    }
    found
}

/// The timeline of `session` among `records`, which are ones purlis draws.
pub fn of(records: &[Record], session: &ChatRef) -> Timeline {
    let tasks = under(records, session);
    let mut lines: Vec<Line> = tasks
        .iter()
        .flat_map(|(record, depth)| lines_of(record, *depth))
        .collect();
    // A stamp sorts as its time, and a record's id as the time it was minted: two lines of
    // one second stand in the order their dispatches started, and a dispatch's own in the
    // order it kept them.
    lines.sort_by(|a, b| (&a.at, &a.dispatch, a.n).cmp(&(&b.at, &b.dispatch, b.n)));
    let unkept = tasks
        .iter()
        .map(|(record, _)| {
            let kept = u32::try_from(record.talk.len()).unwrap_or(u32::MAX);
            record.messages.saturating_sub(kept)
        })
        .fold(0u32, u32::saturating_add);
    Timeline {
        lines,
        unkept,
        refused: 0,
    }
}

/// The timeline of `session` in the project at `root`: its tasks and everything under them,
/// in time order, from the records purlis draws ([`dispatchrecord::drawn`]).
pub fn timeline(root: &Path, session: &ChatRef) -> Timeline {
    let drawn = dispatchrecord::drawn(root);
    Timeline {
        refused: drawn.refused,
        ..of(&drawn.records, session)
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
