//! A task that did not start (#1497, spec #1483, ruling V100-51).
//!
//! A dispatch can be let through and still start no chat: the profile's program is missing or
//! does not answer as its harness, the sandbox will not wrap it, there is no pseudo-terminal,
//! the folder or the workspace is gone, its worktree could not be cut, or a limit filled
//! between the person's Allow and the start.
//!
//! **Such a task is a finished one, failed, under the chat that asked**, and that chat is told
//! as it is told any task's report.
//!
//! **A launch that could not start a task again ends nothing** (D-1497-14). The reason is a
//! condition of the machine at that moment: the task stays recorded, is drawn under the chat
//! that asked with the reason ([`NotPutBack`]), is tried again at the next launch, and the
//! chat that asked is answered a standing and no report ([`waiting_on_the_person`]). It is
//! failed, and that chat told, only when the person ends it ([`not_put_back`]).
//!
//! This module is the rules for it, with no app in it:
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
//! is for a start nobody is on the line for (the person's Allow, and the person ending a task
//! a launch could not start again).

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

/// The most of a reason the sentence carries, in bytes: far more than any refusal purlis
/// writes, and inside what a report may hold with the sentence's own words around it.
const MOST_REASON_BYTES: usize = 3600;

/// What stands in for a reason that cannot be drawn.
const NO_REASON: &str = "purlis could not write down why; the app's log has it";

/// **The sentence**: `it did not start: <reason>`. The reason is `why` on one line with
/// nothing in it that is not a character, in full where it fits a report.
///
/// **A long reason is cut, never replaced**: at a character, with `…`, whatever script it is
/// written in.
pub fn said(why: &str) -> String {
    let mut reason = crate::shown::one_line(why.trim(), usize::MAX);
    if reason.len() > MOST_REASON_BYTES {
        let mut end = MOST_REASON_BYTES;
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        reason.truncate(end);
        reason.push('…');
    }
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

/// **What a chat a launch could not put back is, to the chat that asked for it**: read from
/// its own record of who started it. It decides where the chat is drawn and what the person
/// is offered, and ends nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotPutBack {
    /// A task that still owed its report: drawn under the chat that asked, with the reason,
    /// and kept to be tried again. Failed only if the person ends it.
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
///
/// **One row a task, however often it was tried**: the asking chat's earlier rows for a task
/// of the same name that did not start, not yet cleared, are cleared here, and this record
/// counts them ([`Record::attempts`]) and says the latest reason.
pub fn record(
    root: &Path,
    id: Option<String>,
    opening: Opening,
    why: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<Record> {
    let earlier: Vec<Record> = dispatchrecord::list(root)
        .into_iter()
        .filter(|record| {
            dispatchrecord::never_a_chat(record)
                && !record.cleared
                && record.mode == dispatchrecord::Mode::Task
                && record.task.is_some()
                && record.task == opening.task
                && dispatchrecord::same_chat(&record.asker.chat, &opening.asker.chat)
        })
        .collect();
    let opened = match id {
        Some(id) => dispatchrecord::open_as(root, id, opening, now),
        None => dispatchrecord::open(root, opening, now),
    }?;
    let attempts = earlier
        .iter()
        .map(|record| record.attempts.max(1))
        .sum::<u32>()
        .saturating_add(1);
    dispatchrecord::did_not_start(root, &opened.id, why, attempts, now)?;
    // Only once the new row is kept: a write that failed leaves the earlier ones standing.
    for record in &earlier {
        dispatchrecord::clear(root, &record.id)?;
    }
    dispatchrecord::read(root, &opened.id)
        .ok_or_else(|| io::Error::other("the record of a task that did not start was not kept"))
}

/// **The person ended a task a launch could not start again**: its running record `record`
/// ends failed, saying `why`, and keeps the `conversation` the task was in where one is
/// known, which is what Reopen on its row resumes. `false` for a record that has already
/// ended.
///
/// **A launch never calls this.** A start refused at a launch is a condition of the machine
/// at that moment, and the task is kept and tried again; it ends only on the person's word.
pub fn not_put_back(
    root: &Path,
    record: &Record,
    why: &str,
    conversation: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<bool> {
    if !dispatchrecord::did_not_start(root, &record.id, why, 1, now)? {
        return Ok(false);
    }
    // The record has ended, whatever becomes of this: a conversation that could not be
    // written costs the row its Reopen, and never the ending.
    if let Some(conversation) = conversation {
        let _ = dispatchrecord::ended_in(root, &record.id, conversation);
    }
    Ok(true)
}

/// What the chat that asked is answered about a task a launch could not start again, while
/// it waits on the person: a standing, as a task waiting on the person has one, and no report.
pub fn waiting_on_the_person(why: &str) -> String {
    format!(
        "waiting on the person: it did not start again ({})",
        crate::shown::one_line(why.trim(), 600)
    )
}

/// How a task that did not start is listed for the chat that asked.
pub const LISTED: &str = "did not start";

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
