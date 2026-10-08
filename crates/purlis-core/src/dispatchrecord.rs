//! The dispatch record: one file for each dispatch, kept by the app (#1452, spec #1434).
//!
//! A **dispatch** is one chat starting another: a task, which reports back, or a handoff, where
//! the work moves. Each leaves one record of who asked, which persona it went to, where it
//! worked, the brief, the report, when it started and ended, how often it needed the person,
//! and what its harness said it cost.
//!
//! # Where it lives, and why not in the dispatch log
//!
//! `<state>/app/dispatches/<id>.json`, one file a dispatch, in the app's own state
//! ([`crate::names::state`]): **Clone state, never committed**. A brief and a report are a
//! chat's words about the work, and can hold whatever the work held, so they stay on this
//! machine. The committed dispatch log (`personas/_dispatch/`, [`crate::dispatch`]) keeps its
//! handoff row of four fields, and names nothing this record holds. `purlis persona stats`
//! counts a persona's dispatches from both ([`tally`]).
//!
//! **Kept from git, and from the project's sandboxed chats** (D-1452-11). A sandboxed chat can
//! neither read nor write the store: [`crate::sandbox`] denies it for both, as it denies the
//! hook spool, so a brief or a report written for one persona is not readable by a chat
//! running as another. A chat gets its own dispatch's report on the delivery path and its own
//! list from the app's answer, never from the file. Nothing a chat runs reads it: the readers
//! are the app and the sweep the app runs when it opens a project. **Where there is no
//! sandbox** (a project without one, a chat started without it) only the file's mode holds,
//! and that keeps out other users of the machine, not the person's own chats.
//!
//! It is kept the way a chat's per-session files are ([`crate::retention`]): 30 days after it
//! was last written it is collected when the project is opened, unless the chat that asked or
//! the chat that worked is one the reopen record brings back. A chat is matched by its ULID
//! ([`same_chat`]): a number is dealt again in another launch, and keeps nothing.
//!
//! # Who writes it
//!
//! **The app, and nothing else.** [`open`], [`note`] and [`close`] are called by the app with
//! facts from its own record of the chats: no line on the hook channel names a record, an
//! asker, an outcome or a cost. Three things in a record are still a chat's. The brief and the
//! report's text are its words, stored as written and held to a cap ([`cut`]). And what its
//! harness reported of tokens and cost ([`crate::usage::spent`]) is relayed by the chat's
//! status line through a file a chat can write: **a chat can alter that figure**, so it is
//! shown as reported and decides nothing: the person's optional token limit (#1512) is shown
//! against it and not enforced. It is absent where the harness reports none, and never a
//! zero.
//!
//! A record is one JSON object, written whole and private ([`crate::rewrite::replace`]) under
//! purlis's lock on the directory. A closed record takes no further report: the first
//! [`close`] is the dispatch's end.
//!
//! # What is read back
//!
//! The store is the app's to write, and a file in it is still whatever is on the disk. So a
//! record is put on the screen only where it reads as one the app would have written
//! ([`sound`], [`drawn`]): refused and counted otherwise, never tidied and shown.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The record's version: a file of another is listed by nobody and left as it is.
pub const VERSION: u32 = 1;

/// The store's directory under the state folder's `app/`.
pub const DIR_NAME: &str = "dispatches";

/// What a chat that ended owing a report is recorded as having said (spec #1434: a persona
/// chat that ends without a report yields "failed: ended without a report").
pub const ENDED_WITHOUT_A_REPORT: &str = "ended without a report";

/// The store of the project at `root`.
pub fn dir(root: &Path) -> PathBuf {
    crate::names::state(root).join("app").join(DIR_NAME)
}

/// Whether a dispatch expects a report, or moves the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The persona chat sends a report back to the asking chat.
    Task,
    /// The work moves to the persona chat.
    Handoff,
}

/// A chat, as the app's record of it names it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChatRef {
    /// The app's number for it when the dispatch started.
    pub chat: u32,
    /// Its ULID (ADR 0066), which it keeps across a relaunch and a restart. `None` for a chat
    /// that was given none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Its name as the person saw it then.
    pub name: String,
    /// The persona it runs as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
}

/// Who asked.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Asker {
    /// The asking chat.
    #[serde(flatten)]
    pub chat: ChatRef,
    /// The workspace it asked from; `None` for the project's root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Whether the person dispatched from that chat's tab themselves, and not the chat.
    #[serde(default)]
    pub by_person: bool,
    /// The asking chat's session record, once it wrote one after this dispatch, by its
    /// project-relative path: what lists the dispatch on that record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_record: Option<String>,
}

/// The persona chat: the chat the dispatch started.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Worker {
    #[serde(flatten)]
    pub chat: ChatRef,
    /// The harness it runs, by the word the project calls it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    /// The harness profile it started on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Its own session record, once it wrote one, by its project-relative path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_record: Option<String>,
}

/// How a dispatch's worktree came to be gone, where purlis took it away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Removed {
    /// purlis found its branch merged into the branch it was cut from, and took the folder
    /// and the branch away.
    Merged,
    /// The same, and git would not delete the branch (the clone was not on the branch it
    /// landed in): the folder is gone and the branch stays.
    MergedBranchKept,
    /// The person discarded it, from the window.
    Discarded,
}

/// A worktree a persona chat worked in, on a branch of its own (#1453): the repo it was cut
/// from, in the dispatch's workspace, and the folder and branch purlis named
/// ([`crate::dispatchplace::piece_name`]). **The app's own record of what it cut**: it is what
/// the report names as the branch, whatever the persona chat says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worktree {
    pub repo: String,
    pub piece: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Set once purlis has taken the worktree away, and how. Absent while it is kept, and for
    /// one removed by other means, which the folder's absence says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<Removed>,
}

/// Where the persona chat worked.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Place {
    /// The workspace; `None` for the project's root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// The folder it started in, relative to the project (`.` for its root), or absolute for
    /// one outside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Its own worktree, where the dispatch gave it one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Worktree>,
}

/// How a dispatch came out, in the report's own word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Blocked,
    Failed,
    /// Its asking chat cancelled the task (#1441): the app's own record of what happened,
    /// whatever the task's chat said. Not a failure of the work.
    Cancelled,
    /// The person stopped the chat before it reported (#1443, #1448): Stop, "Stop them", or
    /// the Close of a task that had not reported. The app's own record, and not a failure of
    /// the work either.
    Stopped,
}

impl Outcome {
    pub fn word(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Stopped => "stopped",
        }
    }
}

/// What a dispatch changed, as its report names it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Changed {
    /// What the report says changed, in the persona chat's own words (`purlis dispatch report
    /// --changed`): stored as written, held to a cap ([`cut`]). Absent where it said nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// The report a dispatch ended with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub outcome: Outcome,
    /// The persona chat's words; or purlis's, for a chat that ended owing one.
    pub text: String,
    #[serde(default)]
    pub changed: Changed,
}

/// What the persona chat's harness said the work cost ([`crate::usage::Spent`]). Every part is
/// absent where the harness did not say it, and the whole is absent for a harness that reports
/// none: never a zero.
pub use crate::usage::Spent as Usage;

/// One dispatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub v: u32,
    /// A ULID, minted by [`open`]: the record's file name.
    pub id: String,
    pub mode: Mode,
    pub asker: Asker,
    /// The persona dispatched to; `None` for a chat started as none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// The persona chat.
    pub worker: Worker,
    /// The task's name, where the dispatch gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub place: Place,
    /// The brief: the message the persona chat started on.
    pub brief: String,
    /// Whether the dispatch asked for a report.
    #[serde(default)]
    pub report_owed: bool,
    /// When it started, UTC ([`crate::dispatch::stamp`]).
    pub started: String,
    /// When it ended; absent while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    /// The report it ended with; absent while it runs, and for one that owed none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<Report>,
    /// How many times the persona chat waited on the person.
    #[serde(default)]
    pub needed_you: u32,
    /// How many messages passed between the two chats after the brief.
    #[serde(default)]
    pub messages: u32,
    /// What the harness said it cost; absent for a harness that reports none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// The conversation the persona chat was in when the dispatch ended, by its harness's id
    /// for it (#1485): what **Reopen** on a finished task resumes. Absent while it runs, and
    /// for a chat whose harness named none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
    /// Whether the finished task's row was taken off its asking chat's list (#1485): by
    /// Clear finished, by Reopen, or because the asking chat closed. The record itself stays,
    /// and is collected as any record is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cleared: bool,
    /// Who ended the dispatch, where it was not its persona chat's own report alone (#1485):
    /// **the app's fact, never read from the report's words**. Absent for a dispatch that
    /// ended with a report its chat sent of its own accord, and for every record written
    /// before this key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_by: Option<EndedBy>,
    /// Which way the person ended it, where they did (#1488): stopped, with the one short
    /// report it was given a turn for, or closed, with none. **The app's fact**, as
    /// `ended_by` is. Absent for every other end, and for a record written before this key: a
    /// task the person ended that names no way is read as closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_way: Option<EndedWay>,
    /// The person took the task's chat over after it reported (#1485): they typed in it, or
    /// began a Smart close of it. purlis does not end such a chat, at its report or at the
    /// next launch. It is a finished row once it is closed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept_open: bool,
    /// Whether the task did not start (#1497): it was let through and no program was started
    /// for it, or a launch could not start it again and the person ended it. Its report is
    /// then purlis's own sentence saying why ([`did_not_start`]), failed. For one that never
    /// ran, its `worker` is the chat it would have been: a number and no id, because no chat
    /// ever had it ([`never_a_chat`]). Absent for every other dispatch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub did_not_start: bool,
    /// How many starts of this task were refused, where it was more than one: the asking chat
    /// tried again and each try failed, and they are one row with the latest reason
    /// ([`crate::didnotstart::record`]). Absent for one.
    #[serde(default, skip_serializing_if = "one_or_none")]
    pub attempts: u32,
    /// **How long the task has worked, in seconds** (#1512): its working time as the app's
    /// clock saw it, never time waiting on the person or on its own tasks, nor time purlis was
    /// not running. Kept here so a task restored after a restart keeps the working time it had,
    /// and what `minutes-per-task` is held to. Absent while it is 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub worked: u64,
    /// **The limit purlis stopped it at** (#1512), beside `ended_by: limit`: the app's own
    /// record of which limit and the figure, which its finished row names. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<crate::dispatchlimits::Reached>,
}

fn is_zero(worked: &u64) -> bool {
    *worked == 0
}

fn one_or_none(attempts: &u32) -> bool {
    *attempts <= 1
}

/// Whether `record` is of a task no chat ever was: it did not start, and its worker has a
/// number and no id. That number is nobody's, so the record is never matched to a chat by it.
pub fn never_a_chat(record: &Record) -> bool {
    record.did_not_start && record.worker.chat.id.is_none()
}

/// Who ended a dispatch, where the app knows it was not the persona chat's own report alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndedBy {
    /// Its chat went without reporting, and purlis said so in its place.
    Unreported,
    /// The person stopped or closed it. Whatever it reported in its last turn, it did not
    /// end by itself.
    Person,
    /// purlis stopped it at a limit the person set (#1512): the time one task may work, or
    /// the tokens its session may use. Whatever it reported in its last turn, it did not end
    /// by itself, and nor did the person end it there and then.
    Limit,
}

/// Which of the two ways the person ended a task (#1488, V100-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndedWay {
    /// Stop and get its report: its turn was ended, and it sent one short report.
    Stopped,
    /// Close now: its program was ended with no report from it. Also what a stop comes to
    /// when the task sends no report in the time it has.
    Closed,
}

impl Record {
    /// Whether the dispatch is still running.
    pub fn running(&self) -> bool {
        self.ended.is_none()
    }
}

/// What the app knows of a dispatch as it starts one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening {
    pub mode: Mode,
    pub asker: Asker,
    pub persona: Option<String>,
    pub worker: Worker,
    pub task: Option<String>,
    pub place: Place,
    pub brief: String,
    pub report_owed: bool,
}

/// Something that happened to a running dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The persona chat came to wait on the person.
    NeededYou,
    /// A message passed between the two chats.
    Message,
}

/// How a dispatch ended.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ending {
    /// Its report; `None` for a dispatch that ends owing none.
    pub report: Option<Report>,
    /// What its harness said it cost; `None` for a harness that reports none.
    pub usage: Option<Usage>,
}

/// A new dispatch's id: a ULID. Minted apart from [`open`] where something is named for the
/// dispatch before its record is written, as a worktree task's folder and branch are (#1453).
pub fn mint() -> String {
    ulid::Ulid::generate().to_string()
}

/// Opens the record of a dispatch that started at `now`, and answers it.
pub fn open(
    root: &Path,
    opening: Opening,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<Record> {
    open_as(root, mint(), opening, now)
}

/// [`open`], under an `id` [`mint`] gave earlier. An id that is not one of [`mint`]'s is
/// refused, and one a record already has is too: a record is written once.
pub fn open_as(
    root: &Path,
    id: String,
    opening: Opening,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<Record> {
    if !an_id(&id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a dispatch record is named by an id purlis minted",
        ));
    }
    let record = Record {
        v: VERSION,
        id,
        mode: opening.mode,
        asker: opening.asker,
        persona: opening.persona,
        worker: opening.worker,
        task: opening.task,
        place: opening.place,
        brief: opening.brief,
        report_owed: opening.report_owed,
        started: crate::dispatch::stamp(now),
        ended: None,
        report: None,
        needed_you: 0,
        messages: 0,
        usage: None,
        conversation: None,
        cleared: false,
        ended_by: None,
        ended_way: None,
        kept_open: false,
        did_not_start: false,
        attempts: 0,
        worked: 0,
        limit: None,
    };
    // As it is stored, so what the caller holds is what a read gives back.
    let record = capped(&record);
    let dir = dir(root);
    crate::rewrite::create_dir_all(&dir)?;
    let _held = crate::rewrite::Lock::on(&dir);
    if read(root, &record.id).is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a dispatch record is written once, and this id has one",
        ));
    }
    write(root, &record)?;
    Ok(record)
}

/// Marks the worktree of dispatch `id` as taken away, and how (#1453). `false` for a record
/// that is not there, names no worktree, or already says how its worktree went: the first
/// word is the one kept. A running dispatch's is never marked: its chat still works there.
pub fn worktree_removed(root: &Path, id: &str, how: Removed) -> io::Result<bool> {
    change(root, id, |record| {
        if record.running() {
            return false;
        }
        match record.place.worktree.as_mut() {
            Some(tree) if tree.removed.is_none() => {
                tree.removed = Some(how);
                true
            }
            _ => false,
        }
    })
}

/// Appends `event` to the running dispatch `id`. `false` for a record that is not there or has
/// ended: an ended dispatch counts nothing more.
pub fn note(root: &Path, id: &str, event: Event) -> io::Result<bool> {
    change(root, id, |record| {
        if !record.running() {
            return false;
        }
        match event {
            Event::NeededYou => record.needed_you = record.needed_you.saturating_add(1),
            Event::Message => record.messages = record.messages.saturating_add(1),
        }
        true
    })
}

/// Closes dispatch `id` at `now`. `false` for a record that is not there or has already
/// ended: a dispatch ends once, and its first ending is the one kept.
pub fn close(
    root: &Path,
    id: &str,
    ending: Ending,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<bool> {
    close_by(root, id, ending, None, now)
}

/// [`close`], saying who ended it where that was not its chat's own report alone
/// ([`EndedBy`]): written in the same write as the ending, by the app, from what the app did.
pub fn close_by(
    root: &Path,
    id: &str,
    ending: Ending,
    by: Option<EndedBy>,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<bool> {
    close_as(root, id, ending, by, None, now)
}

/// [`close_by`], saying which way the person ended it where they did ([`EndedWay`]). A way is
/// kept only beside `by: person`: no other end has one.
pub fn close_as(
    root: &Path,
    id: &str,
    ending: Ending,
    by: Option<EndedBy>,
    way: Option<EndedWay>,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<bool> {
    change(root, id, |record| {
        if !record.running() {
            return false;
        }
        record.ended_by = by;
        record.ended_way = way.filter(|_| matches!(by, Some(EndedBy::Person | EndedBy::Limit)));
        record.ended = Some(crate::dispatch::stamp(now));
        record.report = ending.report.as_ref().map(capped_report);
        record.usage = ending.usage.filter(|usage| !usage.is_empty());
        true
    })
}

/// **Dispatch `id` has worked `secs` seconds in all** (#1512): the app's clock keeps it, so a
/// task restored after a restart keeps its working time. `false` for a record that is not
/// there, has ended, or already says it.
pub fn worked(root: &Path, id: &str, secs: u64) -> io::Result<bool> {
    change(root, id, |record| {
        if !record.running() || record.worked == secs {
            return false;
        }
        record.worked = secs;
        true
    })
}

/// **purlis is stopping dispatch `id` at `reached`** (#1512): kept before its end, so its
/// finished row says which limit. `false` for a record that is not there or has ended.
pub fn stopped_at(
    root: &Path,
    id: &str,
    reached: crate::dispatchlimits::Reached,
) -> io::Result<bool> {
    change(root, id, |record| {
        if !record.running() {
            return false;
        }
        record.limit = Some(reached);
        true
    })
}

/// Dispatch `id` ended with its persona chat in `conversation` (#1485): kept, so the finished
/// task can be reopened on it. `false` for a record that is not there, is still running, or
/// already names the same one. Held to a name's cap, as every id a record keeps is.
pub fn ended_in(root: &Path, id: &str, conversation: &str) -> io::Result<bool> {
    let conversation = cut(conversation, MOST_NAME_BYTES);
    change(root, id, |record| {
        if record.running() || record.conversation.as_deref() == Some(conversation.as_str()) {
            return false;
        }
        record.conversation = Some(conversation);
        true
    })
}

/// **The task of dispatch `id` did not start** (#1497): its record ends here, failed, with
/// purlis's own sentence saying `why` ([`crate::didnotstart::said`]), and is marked so. `false`
/// for a record that is not there or has already ended: a dispatch ends once.
///
/// A record opened a moment ago for a task whose start was refused, or the running record of
/// a task a launch could not start again that the person ended: either way the row that stays
/// under the chat that asked is read from it, as every finished row is ([`finished_for`]).
/// `attempts` is how many refused starts the row stands for ([`Record::attempts`]).
pub fn did_not_start(
    root: &Path,
    id: &str,
    why: &str,
    attempts: u32,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<bool> {
    let report = capped_report(&Report {
        outcome: Outcome::Failed,
        text: crate::didnotstart::said(why),
        changed: Changed::default(),
    });
    change(root, id, |record| {
        if !record.running() {
            return false;
        }
        record.ended = Some(crate::dispatch::stamp(now));
        record.report = Some(report);
        record.did_not_start = true;
        record.attempts = if attempts > 1 { attempts } else { 0 };
        true
    })
}

/// How a finished task ended, as its row says it (#1485, V100-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finished {
    Done,
    Cancelled,
    Blocked,
    Failed,
    /// Its program ended before it reported, and purlis said so in its place.
    EndedWithoutAReport,
    /// The person stopped it, and it sent the one short report it was given a turn for.
    StoppedByThePerson,
    /// The person closed it: its program was ended with no report from it.
    ClosedByThePerson,
    /// purlis stopped it at its time limit (#1512), with or without the one short report it
    /// was given a turn for.
    StoppedAtItsTimeLimit,
    /// purlis stopped it with the task above it, which reached its time limit (#1512).
    StoppedWithTheTaskAbove,
}

impl Finished {
    /// How `record`'s task finished; `None` for a handoff, a dispatch still running, and one
    /// that ended owing no report.
    pub fn of(record: &Record) -> Option<Self> {
        if record.mode != Mode::Task {
            return None;
        }
        let report = record.report.as_ref()?;
        // **Who ended it is the app's fact** ([`EndedBy`]), never the report's words: a
        // task cannot make its row read as purlis's word by reporting purlis's sentence, and
        // a task the person stopped is said so whatever its last report says of itself.
        match record.ended_by {
            Some(EndedBy::Person) => {
                return Some(match record.ended_way {
                    Some(EndedWay::Stopped) => Self::StoppedByThePerson,
                    Some(EndedWay::Closed) | None => Self::ClosedByThePerson,
                });
            }
            Some(EndedBy::Unreported) => return Some(Self::EndedWithoutAReport),
            Some(EndedBy::Limit) => {
                return Some(match record.limit {
                    Some(crate::dispatchlimits::Reached::Above { .. }) => {
                        Self::StoppedWithTheTaskAbove
                    }
                    _ => Self::StoppedAtItsTimeLimit,
                });
            }
            None => {}
        }
        Some(match report.outcome {
            Outcome::Done => Self::Done,
            Outcome::Cancelled => Self::Cancelled,
            Outcome::Blocked => Self::Blocked,
            Outcome::Stopped => Self::ClosedByThePerson,
            Outcome::Failed => Self::Failed,
        })
    }

    /// The word on its row, which the person reads: the two ends they caused are said to
    /// them as theirs (#1488).
    pub fn word(self) -> &'static str {
        match self {
            Self::StoppedByThePerson => "stopped by you",
            Self::ClosedByThePerson => "closed by you",
            other => other.said_to_a_chat(),
        }
    }

    /// The same end as a chat is told it, where it lists its tasks: the person is not the
    /// reader there.
    pub fn said_to_a_chat(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::EndedWithoutAReport => ENDED_WITHOUT_A_REPORT,
            Self::StoppedByThePerson => "stopped by the person",
            Self::ClosedByThePerson => "closed by the person",
            Self::StoppedAtItsTimeLimit => "stopped at its time limit",
            Self::StoppedWithTheTaskAbove => "stopped with the task above it",
        }
    }

    /// The value a window is sent for it, and a chat's row is told as its outcome: one word
    /// with no spaces, never drawn.
    pub fn key(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::EndedWithoutAReport => "unreported",
            Self::StoppedByThePerson => "stopped_by_person",
            Self::ClosedByThePerson => "closed_by_person",
            Self::StoppedAtItsTimeLimit => "stopped_at_limit",
            Self::StoppedWithTheTaskAbove => "stopped_with_above",
        }
    }

    /// Whether its row folds into the one "Finished (n)" line: a task that came out done, or
    /// that its asking chat cancelled. **Every other end stays a row of its own until it is
    /// cleared** (V100-9), so a failure is never hidden behind a count. A task that reported
    /// itself blocked is one of those: it is waiting on something, and says what.
    pub fn folds(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
}

/// **The finished tasks listed under the chat `asker`** (#1485), oldest first: every task it
/// asked for whose dispatch has ended with a report and whose row has not been cleared, of the
/// records purlis draws ([`sound`]). Read from the store, so the list is the same after the
/// app is started again.
///
/// A task whose chat is still open is not finished yet, whatever its record says: `open`
/// answers for the persona chats the app has open now, and those are left out. Its report is
/// in, and purlis is about to end its program.
pub fn finished_for(root: &Path, asker: &ChatRef, open: impl Fn(&ChatRef) -> bool) -> Vec<Record> {
    finished(root, open)
        .into_iter()
        .filter(|record| asked_by(record, asker))
        .collect()
}

/// Whether the finished task `record` is listed under `asker`: **by the asking chat's id and
/// never by its number**. A number is dealt again in another launch, so a record that names
/// its asking chat by number alone is listed under nobody: its row would be another chat's.
pub fn asked_by(record: &Record, asker: &ChatRef) -> bool {
    record.asker.chat.id.is_some() && record.asker.chat.id == asker.id
}

/// [`finished_for`], for every asking chat at once: one read of the store.
pub fn finished(root: &Path, open: impl Fn(&ChatRef) -> bool) -> Vec<Record> {
    let mut finished: Vec<Record> = list(root)
        .into_iter()
        .filter(|record| {
            !record.cleared
                && Finished::of(record).is_some()
                // A task no chat ever was has no chat to be open: the number it was dealt
                // is all its record has of one, and is nobody's (#1497). One a launch could
                // not start again was a chat, and is held to the rule every row is.
                && (never_a_chat(record) || !open(&record.worker.chat))
                && sound(record)
        })
        .collect();
    finished.reverse();
    finished
}

/// The person took the task's chat over after it reported: dispatch `id` is marked, so purlis
/// does not end that chat at the next launch either. `false` for a record that is not there
/// or is marked already.
pub fn kept_open(root: &Path, id: &str) -> io::Result<bool> {
    change(root, id, |record| {
        if record.kept_open {
            return false;
        }
        record.kept_open = true;
        true
    })
}

/// Takes the finished task `id`'s row off its asking chat's list. **The row and nothing
/// else**: the record stays as it is but for the mark. `false` for a record that is not there,
/// is not a finished task's, or is cleared already.
pub fn clear(root: &Path, id: &str) -> io::Result<bool> {
    change(root, id, |record| {
        if record.cleared || Finished::of(record).is_none() {
            return false;
        }
        record.cleared = true;
        true
    })
}

/// The chat `asker` closed: the rows of the finished tasks it asked for go with it (V100-10).
/// How many were cleared. Their records stay.
pub fn clear_for(root: &Path, asker: &ChatRef) -> usize {
    list(root)
        .into_iter()
        .filter(|record| {
            !record.cleared && Finished::of(record).is_some() && asked_by(record, asker)
        })
        .filter(|record| clear(root, &record.id).unwrap_or(false))
        .count()
}

/// The newest task whose persona chat had the id `worker`, where a record names one: what a
/// chat reopened from a finished task is told it was ([`crate::dispatched`]).
pub fn task_worked_by(root: &Path, worker: &str) -> Option<Record> {
    list(root).into_iter().find(|record| {
        record.mode == Mode::Task && record.worker.chat.id.as_deref() == Some(worker)
    })
}

/// A chat wrote a session record at `path` (project-relative): every dispatch it asked for
/// that names none yet is listed on it, and the dispatch it worked on names it as the persona
/// chat's. `chat` is the app's record of the chat that wrote it. How many records changed.
pub fn session_recorded(root: &Path, chat: &ChatRef, path: &str) -> usize {
    let dir = dir(root);
    let _held = crate::rewrite::Lock::on(&dir);
    let mut changed = 0;
    for mut record in list(root) {
        let mut touched = false;
        if same_chat(&record.asker.chat, chat) && record.asker.session_record.is_none() {
            record.asker.session_record = Some(path.to_owned());
            touched = true;
        }
        if !never_a_chat(&record)
            && same_chat(&record.worker.chat, chat)
            && record.worker.session_record.is_none()
        {
            record.worker.session_record = Some(path.to_owned());
            touched = true;
        }
        if touched && write(root, &record).is_ok() {
            changed += 1;
        }
    }
    changed
}

/// Whether `recorded`, a chat as a record names it, is `chat`.
///
/// **By its ULID, which a restart and a relaunch keep, and never by its number where the
/// record has an id.** A chat's number is dealt again in another launch, so a record that
/// names a chat by id is that chat's alone, whatever number another chat now has. Only a
/// record with no id is matched by number, because the number is all it has.
pub fn same_chat(recorded: &ChatRef, chat: &ChatRef) -> bool {
    named(recorded, chat.id.as_deref(), Some(chat.chat))
}

/// [`same_chat`], for a chat known by its id and its number apart.
pub fn named(recorded: &ChatRef, id: Option<&str>, number: Option<u32>) -> bool {
    match recorded.id.as_deref() {
        Some(recorded) => id == Some(recorded),
        None => number == Some(recorded.chat),
    }
}

/// A chat that is open, or that the reopen record brings back: its ULID and its number.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Live {
    pub id: Option<String>,
    pub number: Option<u32>,
}

impl Live {
    /// The chats of a reopen record.
    pub fn of(record: &crate::reopen::Record) -> Vec<Self> {
        record
            .chats
            .iter()
            .map(|chat| Self {
                id: chat.identity.id.clone(),
                number: chat.number,
            })
            .collect()
    }

    /// Whether `recorded`, a chat as a record names it, is this one ([`named`]).
    pub fn is(&self, recorded: &ChatRef) -> bool {
        named(recorded, self.id.as_deref(), self.number)
    }
}

/// Every record of the project at `root` this build reads, newest first, **as stored**: what
/// the app's own bookkeeping works on. A file that is not a record of this version is skipped
/// and left as it is. What is put on the screen is [`drawn`]'s.
pub fn list(root: &Path) -> Vec<Record> {
    let Ok(entries) = std::fs::read_dir(dir(root)) else {
        return Vec::new();
    };
    let mut records: Vec<Record> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| read(root, name.strip_suffix(".json")?))
        .collect();
    // A ULID sorts by the time it was minted.
    records.sort_by(|a, b| b.id.cmp(&a.id));
    records
}

/// The records purlis draws, and how many it will not.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Drawn {
    /// Newest first.
    pub records: Vec<Record>,
    /// Records in the store that do not pass [`sound`]: counted, never shown.
    pub refused: usize,
}

/// The records of the project at `root` that may be put on the screen, newest first, and the
/// count of those that may not ([`sound`]).
pub fn drawn(root: &Path) -> Drawn {
    let (records, refused): (Vec<Record>, Vec<Record>) = list(root).into_iter().partition(sound);
    Drawn {
        records,
        refused: refused.len(),
    }
}

/// The most of a brief a record keeps, in bytes: above what a dispatch may carry.
pub const MOST_BRIEF_BYTES: usize = 16 * 1024;
/// The most of a report's text a record keeps, in bytes.
pub const MOST_REPORT_BYTES: usize = 8 * 1024;
/// The most of a name a record keeps, in bytes: a chat's, a persona's, a task's, a branch's.
pub const MOST_NAME_BYTES: usize = 512;
/// The most of a path a record keeps, in bytes.
pub const MOST_PATH_BYTES: usize = 1024;
/// The most files, and the most commits, a report's changes list.
pub const MOST_LISTED: usize = 100;
/// The room [`cut`]'s mark may take past a cap, which a read allows for.
const MARK_ROOM: usize = 48;

/// `text` held to `most` bytes: as it is where it fits, else cut at a character and marked
/// `[cut at N bytes]`, so what is missing is said and never silent. A text already cut is
/// left as it is.
pub fn cut(text: &str, most: usize) -> String {
    if text.len() <= most + MARK_ROOM {
        return text.to_owned();
    }
    let mut end = most;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{} [cut at {most} bytes]", &text[..end])
}

/// `record` as the store writes it: every text held to its cap ([`cut`]), so **a record is
/// never written larger than it can be read back** and a long brief or report never costs the
/// dispatch its record. Applied to what the app hands in, at [`open`] and to a [`close`]'s
/// report: a record already in the store is written back as it was read, so one that does not
/// pass [`sound`] is never tidied into one that does.
fn capped(record: &Record) -> Record {
    let name = |text: &str| cut(text, MOST_NAME_BYTES);
    let path = |text: &str| cut(text, MOST_PATH_BYTES);
    let chat = |chat: &ChatRef| ChatRef {
        chat: chat.chat,
        id: chat.id.as_deref().map(name),
        name: name(&chat.name),
        persona: chat.persona.as_deref().map(name),
    };
    Record {
        v: record.v,
        id: record.id.clone(),
        mode: record.mode,
        asker: Asker {
            chat: chat(&record.asker.chat),
            workspace: record.asker.workspace.as_deref().map(name),
            by_person: record.asker.by_person,
            session_record: record.asker.session_record.as_deref().map(path),
        },
        persona: record.persona.as_deref().map(name),
        worker: Worker {
            chat: chat(&record.worker.chat),
            harness: record.worker.harness.as_deref().map(name),
            profile: record.worker.profile.as_deref().map(name),
            session_record: record.worker.session_record.as_deref().map(path),
        },
        task: record.task.as_deref().map(name),
        place: Place {
            workspace: record.place.workspace.as_deref().map(name),
            folder: record.place.folder.as_deref().map(path),
            worktree: record.place.worktree.as_ref().map(|tree| Worktree {
                repo: name(&tree.repo),
                piece: name(&tree.piece),
                branch: tree.branch.as_deref().map(name),
                removed: tree.removed,
            }),
        },
        brief: cut(&record.brief, MOST_BRIEF_BYTES),
        report_owed: record.report_owed,
        started: name(&record.started),
        ended: record.ended.as_deref().map(name),
        report: record.report.as_ref().map(capped_report),
        needed_you: record.needed_you,
        messages: record.messages,
        usage: record.usage,
        conversation: record.conversation.as_deref().map(name),
        cleared: record.cleared,
        ended_by: record.ended_by,
        ended_way: record.ended_way,
        kept_open: record.kept_open,
        did_not_start: record.did_not_start,
        attempts: record.attempts,
        worked: record.worked,
        limit: record.limit,
    }
}

/// `report` as the store writes it ([`capped`]).
fn capped_report(report: &Report) -> Report {
    let name = |text: &str| cut(text, MOST_NAME_BYTES);
    let path = |text: &str| cut(text, MOST_PATH_BYTES);
    Report {
        outcome: report.outcome,
        text: cut(&report.text, MOST_REPORT_BYTES),
        changed: Changed {
            said: report
                .changed
                .said
                .as_deref()
                .map(|said| cut(said, MOST_REPORT_BYTES)),
            files: listed(&report.changed.files, &path),
            commits: listed(&report.changed.commits, &name),
            branch: report.changed.branch.as_deref().map(name),
        },
    }
}

/// At most [`MOST_LISTED`] of `all`, each held to its cap by `held`.
fn listed(all: &[String], held: &impl Fn(&str) -> String) -> Vec<String> {
    all.iter().take(MOST_LISTED).map(|one| held(one)).collect()
}

/// Whether `record` is one purlis will put on the screen: every text in it within the cap the
/// store writes it to, and none holding a character that draws as nothing or turns the words
/// around it ([`crate::panel::undrawable`]).
///
/// **Checked on the way in, as a report waiting to be handed back is** ([`crate::handback`]).
/// The store is the app's to write, and a file in it is still whatever is on the disk: a
/// record written by anything else that runs as the person is drawn only if it reads as one
/// the app would have written. **Refused, never stripped**: a name tidied into another is not
/// the name that was stored, and a row that looks like another persona's is the harm. A brief
/// and a report may hold line breaks and tabs, which are plain text; nothing else may.
pub fn sound(record: &Record) -> bool {
    let line = |text: &str, most: usize| {
        text.len() <= most + MARK_ROOM && !text.contains(crate::panel::undrawable)
    };
    let prose = |text: &str, most: usize| {
        text.len() <= most + MARK_ROOM
            && !text
                .chars()
                .any(|c| c != '\n' && c != '\t' && crate::panel::undrawable(c))
    };
    let name = |text: &str| line(text, MOST_NAME_BYTES);
    let path = |text: &str| line(text, MOST_PATH_BYTES);
    let maybe = |text: &Option<String>, ok: &dyn Fn(&str) -> bool| text.as_deref().is_none_or(ok);
    let chat =
        |chat: &ChatRef| name(&chat.name) && maybe(&chat.id, &name) && maybe(&chat.persona, &name);
    an_id(&record.id)
        && chat(&record.asker.chat)
        && maybe(&record.asker.workspace, &name)
        && maybe(&record.asker.session_record, &path)
        && maybe(&record.persona, &name)
        && chat(&record.worker.chat)
        && maybe(&record.worker.harness, &name)
        && maybe(&record.worker.profile, &name)
        && maybe(&record.worker.session_record, &path)
        && maybe(&record.task, &name)
        && maybe(&record.place.workspace, &name)
        && maybe(&record.place.folder, &path)
        && record
            .place
            .worktree
            .as_ref()
            .is_none_or(|tree| name(&tree.repo) && name(&tree.piece) && maybe(&tree.branch, &name))
        && prose(&record.brief, MOST_BRIEF_BYTES)
        && name(&record.started)
        && maybe(&record.ended, &name)
        && maybe(&record.conversation, &name)
        && record.report.as_ref().is_none_or(|report| {
            prose(&report.text, MOST_REPORT_BYTES)
                && report
                    .changed
                    .said
                    .as_deref()
                    .is_none_or(|said| prose(said, MOST_REPORT_BYTES))
                && report.changed.files.len() <= MOST_LISTED
                && report.changed.commits.len() <= MOST_LISTED
                && report.changed.files.iter().all(|file| path(file))
                && report.changed.commits.iter().all(|commit| name(commit))
                && maybe(&report.changed.branch, &name)
        })
}

/// The record of dispatch `id` as stored, where there is one this build reads.
pub fn read(root: &Path, id: &str) -> Option<Record> {
    let path = path_of(root, id)?;
    let file = crate::contain::open_no_link(root, &path).ok()?;
    let found = file.metadata().ok()?;
    crate::reopen::refuse_unusable(&path, &found).ok()?;
    let mut text = String::new();
    {
        use std::io::Read;
        file.take(crate::reopen::MAX_BYTES)
            .read_to_string(&mut text)
            .ok()?;
    }
    parse(&text).filter(|record| record.id == id)
}

/// The running dispatch whose persona chat is `chat`, where there is one: the newest.
pub fn running_for(root: &Path, chat: &ChatRef) -> Option<Record> {
    list(root)
        .into_iter()
        .find(|record| record.running() && same_chat(&record.worker.chat, chat))
}

/// The newest dispatch whose persona chat is `chat`, running or ended.
pub fn latest_for(root: &Path, chat: &ChatRef) -> Option<Record> {
    list(root)
        .into_iter()
        .find(|record| !never_a_chat(record) && same_chat(&record.worker.chat, chat))
}

/// **How many dispatches each persona was given**, by the records this machine keeps: every
/// record that names a persona, a task or a handoff, running or ended, counted once. What
/// `purlis persona stats` adds to the committed log's count ([`crate::dispatch::tally`]).
///
/// **An error where the store is there and cannot be read**, which is what a sandboxed chat
/// finds (D-1452-11): "no records" and "records this process may not read" are two answers,
/// and a count that took the second for the first would call a persona never dispatched. No
/// store at all is nothing counted.
pub fn tally(root: &Path) -> io::Result<std::collections::BTreeMap<String, u64>> {
    let mut counts = std::collections::BTreeMap::new();
    let entries = match std::fs::read_dir(dir(root)) {
        Ok(entries) => entries,
        Err(none) if none.kind() == io::ErrorKind::NotFound => return Ok(counts),
        Err(why) => return Err(why),
    };
    for entry in entries {
        let name = entry?.file_name();
        let Some(id) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
            continue;
        };
        if let Some(persona) = read(root, id).and_then(|record| record.persona) {
            *counts.entry(persona).or_default() += 1;
        }
    }
    Ok(counts)
}

/// The dispatches listed on the session record at `path`: the ones its chat asked for, of
/// those purlis draws ([`drawn`]).
pub fn listed_on(root: &Path, path: &str) -> Vec<Record> {
    let mut made: Vec<Record> = drawn(root)
        .records
        .into_iter()
        .filter(|record| record.asker.session_record.as_deref() == Some(path))
        .collect();
    made.reverse();
    made
}

/// Ends every running dispatch whose persona chat `still_open` does not answer for, at `now`:
/// as [`ENDED_WITHOUT_A_REPORT`], failed, where it owed a report, and with none where it owed
/// none. How many ended.
pub fn settle(
    root: &Path,
    still_open: impl Fn(&ChatRef) -> bool,
    now: chrono::DateTime<chrono::Utc>,
) -> usize {
    list(root)
        .into_iter()
        .filter(|record| record.running() && !still_open(&record.worker.chat))
        .filter(|record| {
            let by = record.report_owed.then_some(EndedBy::Unreported);
            close_by(root, &record.id, ended_unreported(record, None), by, now).unwrap_or(false)
        })
        .count()
}

/// [`settle`] as the app runs it when it opens a project, before any chat starts: the chats
/// still open are the ones the reopen record ([`crate::reopen`]) brings back. A record that
/// cannot be read says nothing about which chats are gone, so nothing is ended.
pub fn settle_on_open(root: &Path, now: chrono::DateTime<chrono::Utc>) -> usize {
    let Ok(reopen) = crate::reopen::read_strictly(root) else {
        return 0;
    };
    let live = reopen.as_ref().map(Live::of).unwrap_or_default();
    settle(root, |worker| live.iter().any(|chat| chat.is(worker)), now)
}

/// How `record`'s dispatch ends when its persona chat goes without reporting: failed, saying
/// so, where a report was owed; with no report where none was.
pub fn ended_unreported(record: &Record, usage: Option<Usage>) -> Ending {
    Ending {
        report: record.report_owed.then(|| Report {
            outcome: Outcome::Failed,
            text: ENDED_WITHOUT_A_REPORT.to_owned(),
            changed: Changed::default(),
        }),
        usage,
    }
}

/// Whether the record `file` holds is one a chat in `live` asked for or worked on: what
/// [`crate::retention`] keeps however old. Matched as [`same_chat`] matches: by the chat's
/// ULID, so a chat that only shares a number with one of long ago keeps nothing. A file that
/// does not read as a record is not kept by this.
pub(crate) fn of_a_live_chat(file: &mut std::fs::File, live: &[Live]) -> bool {
    use std::io::Read;
    let mut text = String::new();
    if file
        .take(crate::reopen::MAX_BYTES)
        .read_to_string(&mut text)
        .is_err()
    {
        return false;
    }
    parse(&text).is_some_and(|record| {
        [&record.asker.chat, &record.worker.chat]
            .iter()
            .any(|recorded| live.iter().any(|chat| chat.is(recorded)))
    })
}

/// Whether `name` is a record's file name: `<ULID>.json`.
pub(crate) fn a_record(name: &str) -> bool {
    name.strip_suffix(".json").is_some_and(an_id)
}

/// Whether `name` is what a record's write leaves behind when it is cut short: the temporary
/// file [`crate::rewrite::replace`] writes beside `<ULID>.json` and renames over it,
/// `.purlis-generated.<ULID>.json.<pid>.<tag>.tmp` (or an older build's
/// `.charter-generated.…`). A write that finished left none. One that was killed first left a
/// file holding a brief, which nothing reads and [`crate::retention`] collects with the
/// records.
pub(crate) fn a_record_s_temp(name: &str) -> bool {
    let prefix = crate::names::GENERATED_TEMP_PREFIX;
    std::iter::once(prefix.write)
        .chain(prefix.reads.iter().copied())
        .chain(prefix.history.iter().copied())
        .filter_map(|prefix| name.strip_prefix(prefix))
        .filter_map(|rest| rest.strip_suffix(".tmp"))
        // `<ULID>.json.<pid>.<tag>`: a record's name, then the two parts a write adds.
        .any(|rest| {
            let mut parts = rest.rsplitn(3, '.');
            let (tag, pid, record) = (parts.next(), parts.next(), parts.next());
            tag.is_some_and(|tag| !tag.is_empty())
                && pid.is_some_and(|pid| !pid.is_empty())
                && record.is_some_and(a_record)
        })
}

fn an_id(id: &str) -> bool {
    ulid::Ulid::from_string(id).is_ok_and(|read| read.to_string() == id)
}

/// The file of dispatch `id`, or `None` for an id that is not one [`open`] mints: the only
/// names this store reads or writes.
fn path_of(root: &Path, id: &str) -> Option<PathBuf> {
    an_id(id).then(|| dir(root).join(format!("{id}.json")))
}

fn parse(text: &str) -> Option<Record> {
    serde_json::from_str::<Record>(text)
        .ok()
        .filter(|record| record.v == VERSION)
}

fn write(root: &Path, record: &Record) -> io::Result<()> {
    let path = path_of(root, &record.id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a dispatch record is named by the id it was opened with",
        )
    })?;
    let text = serde_json::to_string_pretty(record).map_err(io::Error::other)?;
    crate::rewrite::replace(
        root,
        &path,
        format!("{text}\n").as_bytes(),
        crate::rewrite::Mode::Private,
    )
}

/// Reads record `id`, applies `edit`, and writes it back when `edit` says it changed, under
/// the store's lock. `false` for no record, or one `edit` left alone.
fn change(root: &Path, id: &str, edit: impl FnOnce(&mut Record) -> bool) -> io::Result<bool> {
    let dir = dir(root);
    let _held = crate::rewrite::Lock::on(&dir);
    let Some(mut record) = read(root, id) else {
        return Ok(false);
    };
    if !edit(&mut record) {
        return Ok(false);
    }
    write(root, &record).map(|()| true)
}

#[cfg(test)]
#[path = "dispatchrecord_tests.rs"]
mod tests;
