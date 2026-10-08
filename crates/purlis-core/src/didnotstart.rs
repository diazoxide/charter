//! A task that did not start (#1497, spec #1483, ruling V100-51).
//!
//! A dispatch can be let through and still start no chat: the profile's program is missing or
//! does not answer as its harness, the sandbox will not wrap it, there is no pseudo-terminal,
//! the folder or the workspace is gone, its worktree could not be cut, a limit filled between
//! the person's Allow and the start, or a launch could not put the task's chat back.
//!
//! **Such a task is a finished one, failed, under the chat that asked**, and that chat is told
//! as it is told any task's report. This module is the rules for it, with no app in it:
//!
//! - **the sentence** ([`said`]): `it did not start: <reason>`, purlis's own words around the
//!   reason the start gave, in full;
//! - **the record** ([`record`], [`not_put_back`]): the task's dispatch record ends failed
//!   with that sentence and is marked ([`crate::dispatchrecord::Record::did_not_start`]), so
//!   its row is read from the store as every finished row is, stays a row of its own, is
//!   cleared as the others are, and is there after the app is started again;
//! - **the report** ([`report`], [`tell`]): outcome failed, in purlis's voice with the reason
//!   quoted under it, left for the asking chat's next turn and remembered as a closed task's
//!   report is, so a command waiting on the task's number is answered with it at once and the
//!   asking chat is typed the one line a landed report types.
//!
//! **Told once.** A start that fails while the asking chat's own command still waits is
//! answered on that command, as it always was, and gets its row and no second word: [`tell`]
//! is for a start nobody is on the line for (the person's Allow, a launch).

use std::io;
use std::path::Path;

use crate::active::Place;
use crate::dispatchdecision::Mode;
use crate::dispatched::Ledger;
use crate::dispatchrecord::{self, Opening, Record};
use crate::handback::{self, For, Handback};
use crate::reopen::{HandedFrom, Owed};

/// What purlis says of a task that did not start, before the reason.
pub const SAYS: &str = "it did not start";

/// The most of a reason the sentence carries, in characters: far more than any refusal purlis
/// writes, and well inside what a report may hold.
const MOST_REASON_CHARS: usize = 3000;

/// What stands in for a reason that cannot be drawn.
const NO_REASON: &str = "purlis could not write down why; the app's log has it";

/// **The sentence**: `it did not start: <reason>`. The reason is `why` on one line with
/// nothing in it that is not a character, in full where it fits a report.
pub fn said(why: &str) -> String {
    let reason = crate::shown::one_line(why.trim(), MOST_REASON_CHARS);
    let said = format!("{SAYS}: {reason}");
    match crate::handoff::report_summary(&said) {
        Ok(said) if !reason.is_empty() => said,
        _ => format!("{SAYS}: {NO_REASON}"),
    }
}

/// The reason in `text`, where `text` is [`said`]'s sentence: what a reader shows as why.
pub fn reason(text: &str) -> Option<&str> {
    text.strip_prefix(SAYS)?
        .strip_prefix(": ")
        .filter(|reason| !reason.trim().is_empty())
}

/// **What a chat a launch could not put back is, to the chat that asked for it** (V100-63):
/// read from its own record of who started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotPutBack {
    /// A task that still owed its report: it is failed, and the chat that asked is told.
    Failed,
    /// A task that had reported, or was reported for: it had finished already, and its row
    /// says how. Nothing more is said.
    FinishedAlready,
    /// Not a task: a chat the person opened, or one a handoff moved the work to. It stays the
    /// person's to retry or forget.
    ThePersons,
}

impl NotPutBack {
    /// What the chat `from` records as started is, where the chat that asked `is_open`. One
    /// whose asking chat did not come back either has nobody to be a row under, and stays the
    /// person's.
    pub fn of(from: Option<&HandedFrom>, asker_is_open: bool) -> Self {
        match from {
            Some(from) if from.mode == Mode::Task && asker_is_open => match from.report {
                Owed::Due => Self::Failed,
                Owed::Sent | Owed::Failed | Owed::Nothing => Self::FinishedAlready,
            },
            _ => Self::ThePersons,
        }
    }
}

/// **Records a task that was let through and did not start**: its dispatch record is opened
/// as `opening` says (under `id`, where one was minted for its worktree) and ended at once,
/// failed, saying `why`. The record as it was stored.
pub fn record(
    root: &Path,
    id: Option<String>,
    opening: Opening,
    why: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<Record> {
    let opened = match id {
        Some(id) => dispatchrecord::open_as(root, id, opening, now),
        None => dispatchrecord::open(root, opening, now),
    }?;
    dispatchrecord::did_not_start(root, &opened.id, why, now)?;
    dispatchrecord::read(root, &opened.id)
        .ok_or_else(|| io::Error::other("the record of a task that did not start was not kept"))
}

/// **A launch could not put the task `record` back**: its running record ends failed, saying
/// `why`, and keeps the `conversation` the task was in where one is known, which is what
/// Reopen on its row resumes. `false` for a record that has already ended.
pub fn not_put_back(
    root: &Path,
    record: &Record,
    why: &str,
    conversation: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<bool> {
    if !dispatchrecord::did_not_start(root, &record.id, why, now)? {
        return Ok(false);
    }
    if let Some(conversation) = conversation {
        dispatchrecord::ended_in(root, &record.id, conversation)?;
    }
    Ok(true)
}

/// Who a task that did not start is reported to, and as what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task<'a> {
    /// The task's name, as its row has it.
    pub name: &'a str,
    /// The chat that asked, by the name the person sees it under.
    pub asker: &'a str,
    /// Where the chat that asked works.
    pub asked_from: Place,
    /// Where the task would have worked.
    pub place: Place,
    /// Whether the person started it, from the asking chat's tab.
    pub by_person: bool,
}

/// **The report in the task's place**: failed, purlis's own sentence ([`said`]), and nothing
/// that is a chat's words, because no chat ran.
pub fn report(task: &Task<'_>, why: &str) -> Handback {
    Handback {
        from: task.name.to_owned(),
        from_workspace: task.place.clone(),
        to: task.asker.to_owned(),
        to_workspace: task.asked_from.clone(),
        summary: said(why),
        task: Some(handback::Task::unreported(None, task.by_person)),
        answered: None,
        stopped: None,
    }
}

/// **Tells the open chat `asker` that its task did not start**: `report` is left for its next
/// turn, and remembered under the number the task was dealt, `task`, as a closed task's
/// report is. So a wait on that number is answered with it at once
/// ([`Ledger::gone`]), a list shows it, and the asking chat is typed the line a landed report
/// types where it may be typed one ([`Ledger::nudge_step`]).
///
/// Once: the caller tells a task's failed start here or on the command that asked, never both.
pub fn tell(
    root: &Path,
    ledger: &mut Ledger,
    asker: u32,
    task: u32,
    report: &Handback,
) -> io::Result<()> {
    let file = handback::leave_at(root, For::Chat(asker), report)?;
    ledger.reported(task, asker, report.clone(), Some(file));
    // No chat is open under that number and none will be: it is remembered as one that has
    // closed, with its report still waiting to be read.
    ledger.forget(task, Some((asker, report.from.as_str())));
    Ok(())
}

#[cfg(test)]
#[path = "didnotstart_tests.rs"]
mod tests;
