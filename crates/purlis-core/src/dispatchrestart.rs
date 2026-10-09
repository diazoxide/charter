//! Tasks across a restart, and a task whose asking chat has gone (#1513, V100-63, V100-64).
//!
//! A quit and a relaunch are the common case: the person updates purlis while tasks are at
//! work. Who asked whom already outlives it, on each chat's own entry in the record of open
//! chats ([`crate::reopen::HandedFrom`]), and a task that had reported is a finished row
//! ([`crate::dispatched::left_out_at_launch`]). This module is the rest: what a launch does
//! with a task that had **not** reported, and what becomes of a report whose asking chat is
//! gone. It is plain Rust over the two stores, with no clock, socket or terminal of its own.
//!
//! # A task still owing its report, at a launch ([`at_launch`])
//!
//! - **It is brought back on its conversation and told to carry on** ([`CARRY_ON`]): one fixed
//!   sentence of purlis's, as its first message. **Never its brief**: the brief is in the
//!   conversation it resumes, and is sent once.
//! - **One that cannot be told so is not started.** With no conversation to resume, or no
//!   profile to be started on, a fresh chat would need the brief a second time. It has ended by
//!   itself: the chat that asked is told so, in the words a task whose program ended is
//!   reported for, and its dispatch record ends ([`end_at_launch`]).
//! - **One whose start is refused waits, as any chat does.** A refusal is not an end: the
//!   switch that stopped every agent, a profile to approve again, a folder that moved. It stays
//!   in the list of chats that did not start, drawn under the chat that asked, and **Try to
//!   start again** on that row starts it told to carry on, while its dispatch still runs
//!   ([`still_at_work`]). Only **End task** on that row (or Forget, for a chat listed across
//!   the window, [`NotBack::LetGo`]) ends it, and then its asking chat is told.
//! - **A brief is not dispatched a second time** ([`dispatched_before`]). A chat cut off in the
//!   middle of `purlis dispatch` never read its answer, and asks again when it carries on:
//!   the same brief, to the same persona, while the task that brief started before the chat's
//!   current run is still its own and still at work, is refused with that task's number.
//!
//! # What a launch believes
//!
//! **The record of open chats is a file, and a chat may have written to it.** A task is told
//! to carry on, or reported for, only where the app's own dispatch record of it agrees
//! ([`Vouched`]): a running task that owes a report, worked by that chat's id as the persona
//! the dispatch went to, on the profile and in the folder it was started on, and asked for by
//! the id of the chat the entry names. The dispatch store is the one a sandboxed chat can
//! neither read nor write ([`crate::dispatchrecord`]). An entry the store does not vouch for
//! comes back as the chat it was and is told nothing. Nothing here grants anything: a chat is
//! started by the launch as every recorded chat is, on its own profile read again, and a
//! permission given to one run of a chat is not brought back with it.
//!
//! # A report whose asking chat has gone ([`take_back`], [`hand_to`])
//!
//! A task whose asking chat closed works on, and its report is kept: on its dispatch record,
//! where the person reads it, and for the workspace it was asked from, as a report with
//! nowhere to go always was ([`crate::handback`]). The record says it reached no chat
//! ([`crate::dispatchrecord::Undelivered`]). **When the person reopens the chat that asked**,
//! every such report is handed to it, once: taken back from the workspace before the chat
//! starts, so it is not read there a second time, and left for the chat's own next turn.
//! **The record says it is owed until it has been handed over** ([`hand_to`]): a launch that
//! dies while the chat starts, or a hand-over that cannot be written, leaves it owed, and the
//! next reopen is handed it from the record. **And
//! when the report lands after it was reopened**, it goes to the open chat that resumed the
//! one that asked ([`crate::reopen::Identity::resumed_from`]) at once.

use std::io;
use std::path::{Path, PathBuf};

use crate::active::Place;
use crate::dispatchrecord::{self, ChatRef, EndedBy, Ending, Outcome, Report};
use crate::handback::{self, For, Handback};
use crate::reopen::{self, Chat, Mode, Owed};

/// What a task brought back at a launch is told, as its first message: purlis's own sentence,
/// with nothing of any chat's in it. It names no chat and no task, so nothing read off the
/// disk is written into a line a harness is started with.
pub const CARRY_ON: &str = "purlis: purlis was restarted while this task was under way, and \
    this is its conversation brought back. Carry on from where it stopped. The tasks this chat \
    dispatched before the restart are still running: `purlis dispatch list` shows them, so \
    dispatch none of them again. A question this chat asked the chat it works for may have \
    been lost: ask it again if the work waits on the answer. When the work is done, send the \
    report with `purlis dispatch report`, as the brief said.";

/// What a launch makes of the tasks in the record of open chats ([`at_launch`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtLaunch {
    /// The record to put back: `record` without the tasks that cannot be resumed.
    pub back: reopen::Record,
    /// The tasks left out of it, in the record's order: each has ended by itself, and is
    /// reported for ([`end_at_launch`]).
    pub not_resumed: Vec<Chat>,
    /// The tasks that are told to carry on as they start, by chat id.
    carry_on: Vec<String>,
    /// The tasks listed under an asking chat the record brings back, by chat id: the ones
    /// that are reported for where their start is refused.
    listed: Vec<String>,
}

impl AtLaunch {
    /// What `chat` is told as it is started at this launch: [`CARRY_ON`] for a task that
    /// still owes its report and is brought back on its conversation, and nothing for every
    /// other chat.
    pub fn told(&self, chat: &Chat) -> Option<&'static str> {
        let id = chat.identity.id.as_ref()?;
        self.carry_on.contains(id).then_some(CARRY_ON)
    }

    /// Whether the chat with the id `id` is a task that still owes its report to a chat the
    /// record brings back: one that is reported for where it does not start.
    pub fn is_a_listed_task(&self, id: &str) -> bool {
        self.listed.iter().any(|listed| listed == id)
    }
}

/// How far the app's own dispatch store vouches for a recorded chat as a task that still owes
/// its report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vouched {
    /// Not a task that owes a report, or the store does not agree that it is.
    No,
    /// A running task that owes a chat a report, and its asking chat is not in the record.
    Alone,
    /// The same, asked for by a chat the record brings back.
    Under,
    /// A task the person started from a chat's tab: their conversation, which comes back as
    /// the chat it was whatever it can resume.
    Persons,
}

/// What the store says of `chat`, one of `record`'s. **Both sides are matched by id**: the
/// chat that worked, and the chat its entry names as the one that asked; and the entry is the
/// chat the dispatch started ([`its_dispatch`]). An entry that disagrees with the store in any
/// of these is vouched for in nothing.
fn vouched(
    root: &Path,
    record: &reopen::Record,
    running: &[dispatchrecord::Record],
    chat: &Chat,
) -> Vouched {
    let Some(dispatch) = its_dispatch(root, running, chat) else {
        return Vouched::No;
    };
    let Some(from) = chat.from.as_ref() else {
        return Vouched::No;
    };
    if dispatch.asker.by_person || from.by_person {
        return Vouched::Persons;
    }
    let asker = record
        .chats
        .iter()
        .find(|other| other.number == Some(from.chat) && other.number != chat.number);
    match asker {
        None => Vouched::Alone,
        Some(asker)
            if asker.identity.id.is_some() && asker.identity.id == dispatch.asker.chat.id =>
        {
            Vouched::Under
        }
        Some(_) => Vouched::No,
    }
}

/// **The running dispatch that `chat`, an entry of the record of open chats, is the task of**,
/// where the app's own store says so: a task that still owes its report, worked by the
/// entry's id, as the persona the dispatch went to, on the profile it was started on, in the
/// folder it was started in. An entry with no profile is matched on the rest: it cannot be
/// told to carry on, and has ended ([`at_launch`]). `None` for anything else, an entry the
/// store contradicts among them.
fn its_dispatch<'a>(
    root: &Path,
    running: &'a [dispatchrecord::Record],
    chat: &Chat,
) -> Option<&'a dispatchrecord::Record> {
    let from = chat.from.as_ref()?;
    let id = chat.identity.id.as_deref()?;
    if from.mode != Mode::Task || from.report != Owed::Due {
        return None;
    }
    let folder = chat
        .cwd
        .as_deref()
        .map(|cwd| dispatchrecord::folder_of(root, cwd));
    running.iter().find(|one| {
        one.running()
            && one.mode == dispatchrecord::Mode::Task
            && one.report_owed
            && one.worker.chat.id.as_deref() == Some(id)
            && one.persona == chat.persona
            && (chat.profile.is_none() || one.worker.profile == chat.profile)
            && one.place.folder == folder
    })
}

/// **Whether `chat`, waiting to start, is still a task at work** (#1513): the app's own store
/// has its dispatch running and owing its report, as a launch holds it to ([`its_dispatch`]).
/// What **Try to start again** asks before it tells the chat to carry on: a dispatch ended
/// meanwhile, by another hand, leaves a chat that is told nothing.
pub fn still_at_work(root: &Path, running: &[dispatchrecord::Record], chat: &Chat) -> bool {
    its_dispatch(root, running, chat).is_some()
}

/// **What a launch does with the tasks of `record` that had not reported** (#1513): `running`
/// is the app's own dispatch store ([`dispatchrecord::list`]), read once, of the project at
/// `root`.
///
/// A task the store vouches for is told to carry on where it can be: it has a conversation to
/// resume and a profile to be started on, which is the road a first message travels. **One
/// that cannot be is left out and has ended by itself** ([`end_at_launch`]), whether or not
/// its asking chat comes back: started blank, it would owe a report it could not know of. A
/// task the person started from a tab is their conversation, and comes back as the chat it
/// was, as a finished one does ([`crate::dispatched::left_out_at_launch`]).
///
/// **One chat id is one chat**: a second entry with an id an earlier one has is vouched for in
/// nothing, so it can neither be told to carry on nor end the first one's dispatch. The
/// put-back gives it an id of its own.
///
/// Every chat that stays keeps its entry whole: its lineage ([`reopen::HandedFrom`]) is
/// neither read into anything new nor written again here, so what a later change adds to it
/// crosses a restart with no word from this module.
pub fn at_launch(
    root: &Path,
    record: &reopen::Record,
    running: &[dispatchrecord::Record],
) -> AtLaunch {
    let mut carry_on = Vec::new();
    let mut listed = Vec::new();
    let mut not_resumed = Vec::new();
    let mut back = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for chat in &record.chats {
        let first = chat
            .identity
            .id
            .as_ref()
            .is_some_and(|id| seen.insert(id.clone()));
        let stands = if first {
            vouched(root, record, running, chat)
        } else {
            Vouched::No
        };
        let id = chat.identity.id.clone().unwrap_or_default();
        let told = chat.resume.is_some() && chat.profile.is_some();
        match (stands, told) {
            (Vouched::No | Vouched::Persons, _) => {}
            (Vouched::Under | Vouched::Alone, false) => {
                not_resumed.push(chat.clone());
                continue;
            }
            (Vouched::Under, true) => {
                listed.push(id.clone());
                carry_on.push(id);
            }
            (Vouched::Alone, true) => carry_on.push(id),
        }
        back.push(chat.clone());
    }
    // A tab left showing a chat that is not coming back shows its own chat again.
    let gone: Vec<u32> = not_resumed.iter().filter_map(|chat| chat.number).collect();
    for chat in &mut back {
        if chat.shows.is_some_and(|shown| gone.contains(&shown)) {
            chat.shows = None;
        }
    }
    AtLaunch {
        back: reopen::Record {
            chats: back,
            ..record.clone()
        },
        not_resumed,
        carry_on,
        listed,
    }
}

/// A task a launch could not bring back, as [`end_at_launch`] settled it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    /// The chat that asked, by the number its entry names: where `file` waits.
    pub asker: u32,
    /// The number the task's chat had, where its entry says.
    pub task: Option<u32>,
    /// The task's name, as its row had it.
    pub name: String,
    /// What the chat that asked is told.
    pub report: Handback,
    /// The file it waits in for that chat's next turn; none where it was kept for a place.
    pub file: Option<PathBuf>,
}

/// Why a task the launch had has ended without coming back ([`end_at_launch`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotBack {
    /// Its harness named no conversation to bring it back on.
    NoConversation,
    /// It runs on no profile, and so could not be told to carry on.
    NoProfile,
    /// Its start was refused at the launch, and the person let it go (Forget).
    LetGo,
}

/// What a dispatch record says of a task a launch did not bring back: purlis's own words.
pub fn not_brought_back(why: NotBack) -> String {
    let said = match why {
        NotBack::NoConversation => {
            "purlis was restarted, and its harness had named no conversation to bring back"
        }
        NotBack::NoProfile => {
            "purlis was restarted, and it ran on no profile it could be brought back on"
        }
        NotBack::LetGo => {
            "purlis was restarted, it could not be started again, and the person let it go"
        }
    };
    format!("{}: {said}", dispatchrecord::ENDED_WITHOUT_A_REPORT)
}

/// **The task `chat` did not come back after a launch: it has ended by itself** (#1513,
/// V100-63). `asker` is the id of the open chat the entry names as the one that asked, where
/// one is open under that number.
///
/// The chat that asked is told as it is told of a task whose program ended before it
/// reported ([`handback::Task::unreported`]): left for its own next turn where it is open, and
/// otherwise kept for the workspace it asked from and marked on the record, so it is handed
/// over when that chat comes back ([`take_back`]). **Who asked, by name and place, is the
/// store's word**; the entry's number is used only where the open chat under it is the one
/// the store names. The dispatch's record ends in purlis's own word ([`EndedBy::Unreported`])
/// and keeps the conversation the task had, which is what a Reopen of its row resumes. What
/// waited for the task itself goes where a closed chat's goes.
///
/// `None` for an entry that names no asking chat, or whose own dispatch is not running.
/// Nothing is told twice: the record is closed once ([`dispatchrecord::close_by`]).
pub fn end_at_launch(
    root: &Path,
    chat: &Chat,
    asker: Option<&str>,
    why: NotBack,
    now: chrono::DateTime<chrono::Utc>,
) -> io::Result<Option<Ended>> {
    let Some(from) = chat.from.as_ref() else {
        return Ok(None);
    };
    let me = ChatRef {
        chat: chat.number.unwrap_or_default(),
        id: chat.identity.id.clone(),
        ..ChatRef::default()
    };
    // Only by its id: a number is all an entry without one has, and a number is not a chat.
    let Some(dispatch) = me
        .id
        .as_ref()
        .and_then(|_| dispatchrecord::running_for(root, &me))
    else {
        return Ok(None);
    };
    // The record first: it is what makes this once. A launch that dies after it leaves the
    // task ended on its record, where the person reads it, and tells nobody twice.
    let ending = Ending {
        report: Some(Report {
            outcome: Outcome::Failed,
            text: not_brought_back(why),
            changed: dispatchrecord::Changed::default(),
        }),
        usage: None,
    };
    if !dispatchrecord::close_by(root, &dispatch.id, ending, Some(EndedBy::Unreported), now)? {
        return Ok(None);
    }
    if let Some(conversation) = chat.resume.as_ref() {
        dispatchrecord::ended_in(root, &dispatch.id, conversation.as_str())?;
    }
    let name = reopen::shown_name(chat, dispatch.worker.harness.as_deref());
    let by_person = dispatch.asker.by_person;
    let asked_from = dispatch
        .asker
        .workspace
        .clone()
        .map_or(Place::PlaneRoot, Place::Workspace);
    let task = handback::Task::unreported(
        dispatch
            .worker
            .session_record
            .as_deref()
            .and_then(handback::record_path),
        by_person,
    )
    .on_branch(branch_of(&dispatch));
    let report = Handback {
        from: name.clone(),
        from_workspace: chat
            .cwd
            .as_deref()
            .and_then(|cwd| crate::active::workspace_of_tree(root, cwd))
            .map_or_else(|| asked_from.clone(), Place::Workspace),
        to: dispatch.asker.chat.name.clone(),
        to_workspace: asked_from.clone(),
        summary: handback::UNREPORTED.to_owned(),
        task: Some(task),
        answered: None,
        stopped: None,
    };
    let asker_open = asker.is_some() && asker == dispatch.asker.chat.id.as_deref();
    let file = if asker_open {
        Some(handback::leave_at(root, For::Chat(from.chat), &report)?)
    } else if by_person {
        // The person's own task, whose tab chat is gone: the report is theirs, on its record,
        // and no chat's (D-1443-9).
        None
    } else {
        let kept = handback::leave_at(root, For::Place(&asked_from), &report)?;
        let kept = handback::kept_name(root, &kept);
        dispatchrecord::kept_undelivered(root, &dispatch.id, kept.as_deref())?;
        None
    };
    if let Some(number) = chat.number {
        handback::orphan(root, number);
        crate::dispatchtalk::forget(root, number);
    }
    Ok(Some(Ended {
        asker: from.chat,
        task: chat.number,
        name,
        report,
        file,
    }))
}

/// The branch purlis cut for `dispatch`, where it cut one: what a report names as its own.
fn branch_of(dispatch: &dispatchrecord::Record) -> Option<handback::Branch> {
    let tree = dispatch.place.worktree.as_ref()?;
    Some(handback::Branch {
        name: tree.branch.clone()?,
        repo: tree.repo.clone(),
    })
}

/// **The task that `brief`, to `persona`, already started before the restart** (#1513), of
/// `restored`: the running dispatch records of the tasks the asking chat had at work when the
/// app was last quit, which the caller reads from its own record of that chat's tasks.
///
/// The same brief as the store keeps it ([`dispatchrecord::brief_as_kept`]) and the same
/// persona, on a task that still owes its report. **Only a task brought back by this launch
/// is ever in `restored`**: a chat that dispatches one brief twice in one launch is asking
/// for two tasks, and gets them.
pub fn dispatched_before<'a>(
    restored: &'a [dispatchrecord::Record],
    persona: Option<&str>,
    brief: &str,
) -> Option<&'a dispatchrecord::Record> {
    let brief = dispatchrecord::brief_as_kept(brief);
    restored.iter().find(|one| {
        one.running()
            && one.mode == dispatchrecord::Mode::Task
            && one.report_owed
            && one.persona.as_deref() == persona
            && one.brief == brief
    })
}

/// **The dispatches chat `asker` made before its current run began** (#1513): `run` is that
/// run's id, a ULID (ADR 0066), which says to the millisecond when the run began, and so does
/// a dispatch record's own id ([`dispatchrecord::mint`]). Those are the dispatches it may not
/// know the answer to, because it was started again since it asked: what
/// [`dispatched_before`] looks among. None for a run that is not a ULID.
pub fn made_before_its_run(
    records: Vec<dispatchrecord::Record>,
    asker: &ChatRef,
    run: &str,
) -> Vec<dispatchrecord::Record> {
    let began = |id: &str| ulid::Ulid::from_string(id).ok().map(|id| id.timestamp_ms());
    let Some(run) = began(run) else {
        return Vec::new();
    };
    records
        .into_iter()
        .filter(|record| {
            dispatchrecord::asked_by(record, asker) && began(&record.id).is_some_and(|at| at < run)
        })
        .collect()
}

/// What a chat is told when it dispatches a brief that is already running as the task called
/// `name`, in chat `task`, from before its current run: purlis restarted it, a restart gave it
/// a grant, it was started fresh, or it was cleared.
pub fn not_again(name: &str, task: u32) -> String {
    format!(
        "purlis did not dispatch this a second time: the same brief is already running as \
         '{name}' (chat {task}). This chat dispatched it before it was last started again or \
         cleared, and it is still at work. Wait for its report with `purlis dispatch wait {task}`. To run it \
         twice on purpose, give the second one a brief of its own."
    )
}

/// **After a launch, a dispatch is running only where its chat came back or waits to** (#1513):
/// every other running dispatch has ended by itself, failed where it owed a report, as
/// [`dispatchrecord::settle`] ends it. `live` answers for the chats the launch opened and the
/// ones still waiting to start. A task's report owed to a chat that asked, and not the
/// person's own, is kept for that chat should it come back ([`take_back`]). How many ended.
///
/// Opening a project settles what the record of open chats does not bring back; this is what
/// a launch then could not: a record that was not put back (the person chose to start fresh),
/// and a chat it named that was never tried.
///
/// `asker_back` answers whether a task's asking chat is back, or a chat that resumed it: a
/// task whose asking chat is not is cleared and forgotten as it ends
/// ([`dispatchrecord::settle_asked`], #1556).
pub fn settle_after_launch(
    root: &Path,
    live: impl Fn(&ChatRef) -> bool,
    asker_back: impl Fn(&dispatchrecord::Record) -> bool,
    now: chrono::DateTime<chrono::Utc>,
) -> usize {
    dispatchrecord::settle_asked(root, live, asker_back, now)
}

/// **Chat `asker` closed with reports still waiting for its next turn** (#1513): each was moved
/// to its workspace ([`handback::orphan_kept`]), and each that is a task's is marked on its
/// dispatch record as reaching no chat, with the name it is kept under, so the chat that asked
/// is handed it if the person reopens it. `moved` is what the move answered. How many were
/// marked.
///
/// A report is matched to the newest ended task `asker` asked for, called what the report
/// says it is from, whose record does not say so already: by its words for a report, by
/// purlis's mark for one purlis wrote. One that matches no record is left as it is.
///
/// `resumed_from` is the id of the chat `asker` resumed, where it resumed one (a Resume, or a
/// Reopen of a finished task's row): the reports it was handed were asked for by that chat,
/// and their records name it (#1546).
pub fn unread_at_close(
    root: &Path,
    asker: &ChatRef,
    resumed_from: Option<&str>,
    moved: &[(Handback, Option<PathBuf>)],
) -> usize {
    if asker.id.is_none() {
        return 0;
    }
    let mut asked: Vec<dispatchrecord::Record> = dispatchrecord::list(root)
        .into_iter()
        .filter(|record| {
            record.mode == dispatchrecord::Mode::Task
                && !record.running()
                && record.undelivered.is_none()
                && !record.asker.by_person
                && (dispatchrecord::asked_by(record, asker)
                    || resumed_from.is_some_and(|was| record.asker.chat.id.as_deref() == Some(was)))
        })
        .collect();
    let mut marked = 0;
    for (report, kept) in moved {
        let Some(at) = asked
            .iter()
            .position(|record| is_its_report(record, report))
        else {
            continue;
        };
        let record = asked.remove(at);
        let kept = kept
            .as_deref()
            .and_then(|file| handback::kept_name(root, file));
        if dispatchrecord::kept_undelivered(root, &record.id, kept.as_deref()).unwrap_or(false) {
            marked += 1;
        }
    }
    marked
}

/// Whether `report`, left for the chat that asked, is the one `record`'s task ended with.
fn is_its_report(record: &dispatchrecord::Record, report: &Handback) -> bool {
    let name = record
        .task
        .clone()
        .unwrap_or_else(|| record.worker.chat.name.clone());
    let Some(ended) = record.report.as_ref() else {
        return false;
    };
    if report.stopped.as_ref().is_some_and(|stopped| stopped.task) {
        return ended.outcome == Outcome::Stopped
            && report.from == handback::in_purlis_s_line(&name);
    }
    let Some(task) = report.task.as_ref() else {
        return false;
    };
    report.from == name
        && if task.unreported {
            record.ended_by == Some(EndedBy::Unreported)
        } else {
            ended.text == report.summary
        }
}

// ---- a report whose asking chat has gone ------------------------------------------------------

/// One report that reached no chat, taken back to be handed to the chat that asked
/// ([`take_back`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owing {
    /// The dispatch's record.
    pub id: String,
    /// The chat that asked, as the record names it: whose chat it is handed to.
    pub asker: ChatRef,
    /// The report, as the chat that asked is told it.
    pub report: Handback,
}

/// `record`'s report as the chat that asked is told it, rebuilt from the record: its own
/// report, purlis's word that it ended without one, or purlis's word that the person stopped
/// it. `None` for a record with no report, and for one whose words a report may not carry:
/// that one stays on its record, for the person.
pub fn report_of(record: &dispatchrecord::Record) -> Option<Handback> {
    let report = record.report.as_ref()?;
    let name = record
        .task
        .clone()
        .unwrap_or_else(|| record.worker.chat.name.clone());
    let place =
        |workspace: &Option<String>| workspace.clone().map_or(Place::PlaneRoot, Place::Workspace);
    let session_record = record
        .worker
        .session_record
        .as_deref()
        .and_then(handback::record_path);
    let by_person = record.asker.by_person;
    let whole = |from: String, summary: String, task, stopped| Handback {
        from,
        from_workspace: place(&record.place.workspace),
        to: record.asker.chat.name.clone(),
        to_workspace: place(&record.asker.workspace),
        summary,
        task,
        answered: None,
        stopped,
    };
    if report.outcome == Outcome::Stopped {
        return Some(whole(
            handback::in_purlis_s_line(&name),
            String::new(),
            None,
            Some(handback::Stopped {
                wrote: false,
                task: true,
                by_person,
                record: session_record,
                // The record keeps no names of tasks ended with it (#1488).
                below: Vec::new(),
                // A stop at a limit the person set is said as one (#1512).
                limit: record.limit,
            }),
        ));
    }
    if record.ended_by == Some(EndedBy::Unreported) {
        return Some(whole(
            name,
            handback::UNREPORTED.to_owned(),
            Some(
                handback::Task::unreported(session_record, by_person).on_branch(branch_of(record)),
            ),
            None,
        ));
    }
    let outcome = match report.outcome {
        Outcome::Done => handback::Outcome::Done,
        Outcome::Blocked => handback::Outcome::Blocked,
        Outcome::Failed => handback::Outcome::Failed,
        Outcome::Cancelled => handback::Outcome::Cancelled,
        Outcome::Stopped => return None,
    };
    let summary = crate::handoff::report_summary(&report.text).ok()?;
    let changed = match report.changed.said.as_deref() {
        None => None,
        Some(said) => Some(crate::handoff::report_summary(said).ok()?),
    };
    Some(whole(
        name,
        summary,
        Some(handback::Task {
            outcome,
            changed,
            record: session_record,
            by_person,
            unreported: false,
            stepped_in: false,
            branch: branch_of(record),
        }),
        None,
    ))
}

/// **Takes back every report that reached no chat and that `asked` answers for** (#1513,
/// V100-64), oldest first: called before the chat that asked is started again, so the copy
/// kept for its workspace is gone before that chat's start reads the workspace's reports.
/// `asked` is given each record, and answers for its asking chat.
///
/// **Nothing is claimed here.** Each record still says its report is owed until
/// [`hand_to`] has handed it over: a start that never finishes (the app dies in it) leaves the
/// report on its record, owed, and the next reopen is handed it from there. One that cannot
/// be told as a report is left as it is.
pub fn take_back(root: &Path, asked: impl Fn(&dispatchrecord::Record) -> bool) -> Vec<Owing> {
    dispatchrecord::undelivered_to(root, asked)
        .into_iter()
        .filter_map(|record| {
            let report = report_of(&record)?;
            if let Some(name) = record.undelivered.as_ref()?.kept.as_deref() {
                handback::withdraw(root, name);
            }
            Some(Owing {
                id: record.id,
                asker: record.asker.chat,
                report,
            })
        })
        .collect()
}

/// **Hands the reports [`take_back`] took to chat `chat`**, the chat that asked, reopened and
/// started: each is left for its own next turn. Answers how many were left.
///
/// **Left first, then claimed** ([`dispatchrecord::delivered_late`], under the store's lock):
/// the app dying between the two leaves the report with the chat and still owed on its
/// record, never with nobody. Two reopens of one chat hand a report over once between them:
/// the one whose claim comes second takes back what it left. A copy kept for the workspace
/// again meanwhile goes with the claim. **A report that cannot be written for the chat is
/// given back** as a start that did not happen gives it back ([`give_back`]).
pub fn hand_to(root: &Path, owing: &[Owing], chat: u32) -> usize {
    let mut handed = 0;
    for one in owing {
        let owed =
            dispatchrecord::read(root, &one.id).is_some_and(|record| record.undelivered.is_some());
        if !owed {
            continue;
        }
        let Ok(file) = handback::leave_at(root, For::Chat(chat), &one.report) else {
            give_back(root, std::slice::from_ref(one));
            continue;
        };
        match dispatchrecord::delivered_late(root, &one.id) {
            Ok(Some(claimed)) => {
                if let Some(name) = claimed.kept.as_deref() {
                    handback::withdraw(root, name);
                }
                handed += 1;
            }
            // Handed over by another reopen meanwhile: this copy is not the chat's to read.
            _ => handback::took(root, &file),
        }
    }
    handed
}

/// The chat that asked did not start after all: what [`take_back`] took is as it was. Each
/// record still says its report is owed, and **one whose kept copy is gone is kept for its
/// workspace again**: taken by this start, or by a start the app died in before it. A record
/// another reopen handed over meanwhile, and one whose copy is still there, are left as they
/// are.
pub fn give_back(root: &Path, owing: &[Owing]) {
    for one in owing {
        let Some(owed) = dispatchrecord::read(root, &one.id).and_then(|record| record.undelivered)
        else {
            continue;
        };
        let gone = owed
            .kept
            .as_deref()
            .is_some_and(|name| !handback::still_kept(root, name));
        if !gone {
            continue;
        }
        let kept = kept_for_its_workspace(root, one);
        if !dispatchrecord::kept_again(root, &one.id, kept.as_deref()).unwrap_or(false)
            && let Some(name) = kept
        {
            handback::withdraw(root, &name);
        }
    }
}

/// `one`'s report, kept for the workspace it was asked from again: the name it is kept under.
fn kept_for_its_workspace(root: &Path, one: &Owing) -> Option<String> {
    let file = handback::leave_at(root, For::Place(&one.report.to_workspace), &one.report).ok()?;
    handback::kept_name(root, &file)
}

#[cfg(test)]
#[path = "dispatchrestart_tests.rs"]
mod tests;
