//! The app's half of the dispatch record (#1452): writing one for each dispatch it starts, and
//! handing the window the list its Dispatches tab draws.
//!
//! `purlis_core::dispatchrecord` is the store and argues where it lives. This module is the
//! **only caller of its writers**, and every fact it writes comes from the app's own record of
//! the two chats: [`opened`] where the app starts a persona chat, [`needed_you`] where the
//! board puts that chat in the needs-you queue, [`reported`] where the app accepts its report,
//! [`message`] where the app takes a message between the two chats, [`ended`] where the app
//! closes it, and [`session_recorded`] where the app writes a chat's session record. Nothing on the hook channel names a record, so no line a chat sends makes
//! one, points one at another asker or sets its counts. **A report's own line does carry how
//! the task ended**, which is the persona chat's to say, and that outcome is written to the
//! record of the dispatch that chat was started by, and to no other.
//!
//! **The cost is the one figure that is not the app's own, and a chat can alter it.** It is
//! what the persona chat's harness reported through its status line
//! (`purlis_core::usage::spent`), read as the dispatch ends from a file in the project's
//! per-session state, which a chat can write. So the window says *Cost (reported)*, nothing
//! decides anything by it, and it is absent for a harness that reports none. It must be moved
//! out of a chat's reach before any budget reads it (D-1452-12).
//!
//! What the window draws is only what passes `dispatchrecord::sound`: a record in the store
//! holding text purlis refuses to draw is counted and not shown.
//!
//! # A dispatch's worktree (#1453)
//!
//! A dispatch that gave its persona chat a worktree lists it on its row, with how it stands:
//! kept, merged, discarded or gone. **Nothing merges it, ever.** It goes one of two ways, and
//! both are here:
//!
//! - **Discard** ([`dispatch_worktree_discard`]) is a window command, so only the person runs
//!   it: no line on the hook channel reaches it. It asks first
//!   ([`dispatch_worktree_loss`] reads exactly what would go), is refused while any chat
//!   stands in the worktree, and removes the folder only where the paths it holds are still
//!   the ones the person was shown. **It loses no commit**: the branch stays unless git finds it merged
//!   (ADR 0072 §4).
//! - **purlis takes a merged one away itself** ([`tidy_closed`], and
//!   `purlis_core::dispatchplace::tidy_at_open`), looking when its chat is closed and when the
//!   project is opened and at no other time. Only by git's safe removal: a folder holding
//!   anything uncommitted stays exactly as it is.
//!
//! The window says all of this of a **branch** and its **folder**, never of a worktree
//! (ADR 0072 §4), so the sentences here that a person reads do too.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use purlis_core::dispatchrecord::{self, ChatRef, Ending, Event, Opening, Outcome, Record, Taken};

use crate::planes::Held;

/// Which of a project's chats were waiting on the person at their last report, so that
/// [`needed_you`] counts each wait once: the move into the queue, not every report made while
/// in it.
#[derive(Default)]
pub struct Waiting(Mutex<HashSet<u32>>);

impl Waiting {
    /// Chat `session` is `waiting` now. Whether that is a wait that has just begun.
    fn began(&self, session: u32, waiting: bool) -> bool {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if waiting {
            held.insert(session)
        } else {
            held.remove(&session);
            false
        }
    }

    fn forget(&self, session: u32) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&session);
    }
}

/// Chat `session` as a record names it, from the app's own record of the open chats.
pub(crate) fn chat_ref(held: &Held, session: u32) -> Option<ChatRef> {
    let chat = held.chats().recorded_chat(session)?;
    Some(ChatRef {
        chat: session,
        id: chat.identity.id.clone(),
        name: held
            .chats()
            .shown_name(session)
            .unwrap_or_else(|| chat.name.clone()),
        persona: chat.persona,
    })
}

/// Where `cwd` is, as a record says it: relative to the project (`.` for its root), or the
/// path itself for a folder outside it.
pub(crate) fn folder(root: &Path, cwd: &Path) -> String {
    // One rule, which a launch holds a restored task to (#1513).
    dispatchrecord::folder_of(root, cwd)
}

/// The app started a persona chat: its dispatch has a record from now, under `id` where one
/// was minted before the chat started (a worktree task's folder and branch are named for it,
/// #1453). A record that could not be written is said in the app's log and costs the dispatch
/// nothing: the chat is open.
pub(crate) fn opened(held: &Held, id: Option<String>, opening: Opening) {
    let now = chrono::Utc::now();
    let written = match id {
        Some(id) => dispatchrecord::open_as(held.root(), id, opening, now),
        None => dispatchrecord::open(held.root(), opening, now),
    };
    match written {
        // An open Activity tab hears of the dispatch as it starts (#1495).
        Ok(record) => crate::activity::dispatched(held, &record),
        Err(why) => tracing::warn!("purlis: a dispatch's record was not written ({why})"),
    }
}

/// The worktree the app cut for the dispatch chat `session` worked on, where it cut one: this
/// app's record, which is what the task's report names as its branch.
///
/// **Of its newest dispatch, running or ended**: a report sent after the record ended (a
/// follow-up's, a stopped chat's last one) names the branch as the first did.
pub(crate) fn worktree_of(held: &Held, session: u32) -> Option<dispatchrecord::Worktree> {
    dispatchrecord::latest_for(held.root(), &chat_ref(held, session)?)?
        .place
        .worktree
}

/// The branch a report of chat `session` names: the one the app cut for its dispatch, from
/// the record ([`worktree_of`]). For a report a chat sends and for one the app writes in a
/// chat's place alike.
pub(crate) fn branch_of(held: &Held, session: u32) -> Option<purlis_core::handback::Branch> {
    let tree = worktree_of(held, session)?;
    Some(purlis_core::handback::Branch {
        name: tree.branch?,
        repo: tree.repo,
    })
}

/// **How each task ended, by its dispatch record, for one reading of the rows** (#1484).
///
/// The store is listed at most once, and only when a row asks: a sidebar with no settled task
/// reads nothing, and one with twenty reads the store once, not twenty times.
pub(crate) struct Outcomes<'a> {
    held: &'a Held,
    listed: Option<Vec<Record>>,
}

impl<'a> Outcomes<'a> {
    pub(crate) fn of(held: &'a Held) -> Self {
        Self { held, listed: None }
    }

    /// How task `session` ended, in its record's word, where its newest dispatch ended with a
    /// report: its own, or the one the app wrote in its place. The app's record, which a
    /// restart of the app keeps.
    pub(crate) fn of_task(&mut self, session: u32) -> Option<&'static str> {
        let me = chat_ref(self.held, session)?;
        let root = self.held.root();
        let listed = self
            .listed
            .get_or_insert_with(|| dispatchrecord::list(root));
        let newest = listed.iter().find(|record| {
            // A task no chat ever was is nobody's by its number (#1497).
            !dispatchrecord::never_a_chat(record)
                && dispatchrecord::same_chat(&record.worker.chat, &me)
        })?;
        // **A task the person ended says which way** (#1488), by the app's own fact and never
        // its report's word: `stopped_by_person` or `closed_by_person`. Every other end is its
        // record's outcome word, as it was.
        Some(match dispatchrecord::Finished::of(newest) {
            Some(
                how @ (dispatchrecord::Finished::StoppedByThePerson
                | dispatchrecord::Finished::ClosedByThePerson),
            ) => how.key(),
            _ => newest.report.as_ref()?.outcome.word(),
        })
    }
}

/// What a chat's list of its tasks says of the branch task `session` was given, where it was
/// given one: the branch, and how it stands (`purlis_core::dispatchplace::Standing::said`).
pub(crate) fn branch_listed(held: &Held, session: u32) -> Option<(String, String)> {
    let record = dispatchrecord::latest_for(held.root(), &chat_ref(held, session)?)?;
    let branch = record.place.worktree.as_ref()?.branch.clone()?;
    let stands = purlis_core::dispatchplace::standing(held.root(), &record)?;
    Some((branch, stands.said().to_owned()))
}

/// The board took a report from chat `session`: where it has just come to wait on the person,
/// and it is a persona chat with a running dispatch, that dispatch needed the person once
/// more.
pub(crate) fn needed_you(held: &Held, session: u32) {
    let waiting = held.hooks().board().needs_you().contains(&session);
    if !held.dispatches().began(session, waiting) {
        return;
    }
    // Only a chat a dispatch opened has a record to count on, and the app's own record of the
    // chat says so without reading the store.
    if held.chats().handed_from(session).is_none() {
        return;
    }
    note(held, session, Event::NeededYou);
}

/// `event` on the running dispatch whose persona chat is `session`, where there is one.
pub(crate) fn note(held: &Held, session: u32, event: Event) {
    let Some(record) = running_for(held, session) else {
        return;
    };
    if let Err(why) = dispatchrecord::note(held.root(), &record.id, event) {
        tracing::warn!("purlis: a dispatch's record was not updated ({why})");
    }
}

/// The app took `said`, a message between persona chat `task` and the chat that asked for it:
/// a follow-up, a progress note, a question or an answer. Its dispatch counts one more, and
/// keeps the message's text for the session's Activity (#1495), in one write. Called only once
/// the message is left for its reader, with the app's own record of which chat is the task: no
/// line names a record.
pub(crate) fn message(held: &Held, task: u32, said: &purlis_core::dispatchtalk::Message) {
    kept(held, task, said.kind, None, &said.text, None);
}

/// [`message`], for the asking chat's answer to question `number` of `task` (#1496): the
/// window is told which question the line answers, so an open Activity tab closes that one
/// and no other.
pub(crate) fn answered(
    held: &Held,
    task: u32,
    said: &purlis_core::dispatchtalk::Message,
    number: u32,
) {
    kept(held, task, said.kind, None, &said.text, Some(number));
}

/// One message on `task`'s running dispatch: counted, its text kept, and the window told.
/// `by` is who said it where that is not the chat its kind names, and `answers` the number of
/// the question it answered, where it is an answer.
fn kept(
    held: &Held,
    task: u32,
    kind: purlis_core::dispatchtalk::Kind,
    by: Option<dispatchrecord::By>,
    text: &str,
    answers: Option<u32>,
) {
    let Some(record) = running_for(held, task) else {
        return;
    };
    let now = chrono::Utc::now();
    match dispatchrecord::said_by(held.root(), &record.id, kind, by, text, now) {
        Ok(Taken::Kept(kept)) => crate::activity::said(held, &kept, answers),
        // Counted, and past what a record keeps the text of: the tab is told how many, and
        // none of the message's words.
        Ok(Taken::Counted(counted)) => crate::activity::unkept(held, &counted, answers),
        Ok(Taken::Nothing) => {}
        Err(why) => tracing::warn!("purlis: a dispatch's record was not updated ({why})"),
    }
}

/// The person answered the question persona chat `task` put to its asking chat (#1496), with
/// `text`: its dispatch counts one more message and keeps the answer **with who said it**, so
/// the session's Activity says it was the person's and not the asking chat's. Called only from
/// the window's own command, once the answer is taken.
pub(crate) fn person_answered(held: &Held, task: u32, text: &str, number: u32) {
    kept(
        held,
        task,
        purlis_core::dispatchtalk::Kind::Answer,
        Some(dispatchrecord::By::Person),
        text,
        Some(number),
    );
}

/// The app accepted chat `session`'s report: its dispatch ends with it. `outcome` and `text`
/// are the report's, and `changed` is what it says changed, where it says; what it cost is
/// read from the app's record of the chat.
///
/// **The window is told the rows changed, once the record is closed** (#1484): every caller has
/// just settled what the chat owed, and its row says how it ended from the record this closes.
/// Told after, so the read the window then makes finds the outcome.
pub(crate) fn reported(
    held: &Held,
    session: u32,
    outcome: Outcome,
    text: &str,
    changed: Option<&str>,
    // Who ended it, where that was not the chat's own report alone (#1485): the person's
    // stop, or purlis saying it went without one.
    by: Option<dispatchrecord::EndedBy>,
    // Which way the person ended it, where they did (#1488).
    way: Option<dispatchrecord::EndedWay>,
) {
    close_with_report(held, session, outcome, text, changed, by, way);
    // A report that reached no chat goes to the chat that resumed the one that asked (#1513).
    crate::restored::to_a_resumed_asker(held, session);
    held.rows_changed();
}

fn close_with_report(
    held: &Held,
    session: u32,
    outcome: Outcome,
    text: &str,
    changed: Option<&str>,
    by: Option<dispatchrecord::EndedBy>,
    way: Option<dispatchrecord::EndedWay>,
) {
    let Some(record) = running_for(held, session) else {
        return;
    };
    let ending = Ending {
        report: Some(dispatchrecord::Report {
            outcome,
            text: text.to_owned(),
            // What it says changed is the chat's own words, kept as said (#1452). The branch
            // is the one the app cut for it (#1453), from this record and never from the
            // report.
            changed: dispatchrecord::Changed {
                said: changed.map(str::to_owned),
                branch: record
                    .place
                    .worktree
                    .as_ref()
                    .and_then(|tree| tree.branch.clone()),
                ..dispatchrecord::Changed::default()
            },
        }),
        usage: spent(held, session),
    };
    close(held, &record, ending, by, way);
    ended_in(held, session, &record);
}

/// Dispatch `record` has ended, with its persona chat `session` in the conversation the app
/// follows it in: kept on the record, which is what a Reopen of the finished task resumes
/// (#1485). Asked while the app still knows the chat.
fn ended_in(held: &Held, session: u32, record: &Record) {
    let Some(conversation) = conversation_of(held, session) else {
        return;
    };
    if let Err(why) = dispatchrecord::ended_in(held.root(), &record.id, &conversation) {
        tracing::warn!("purlis: a dispatch's record was not given its conversation ({why})");
    }
}

/// The conversation chat `session` is in: the one the board follows it in, else the one the
/// app started it on.
pub(crate) fn conversation_of(held: &Held, session: u32) -> Option<String> {
    held.board().conversation(session).or_else(|| {
        held.chats()
            .recorded_chat(session)
            .and_then(|chat| chat.resume)
            .map(|id| id.as_str().to_owned())
    })
}

/// The app is closing chat `session`: a dispatch it was still working on ends here, failed and
/// saying so where it owed a report, with none where it owed none. Asked before the chat is
/// taken off the board, while the app still knows its conversation.
///
/// **A chat started again in its place is the same chat** (its ULID, ADR 0066): where another
/// open chat carries it, the dispatch carries on.
pub(crate) fn ended(held: &Held, session: u32) {
    held.dispatches().forget(session);
    if held.chats().handed_from(session).is_none() {
        return;
    }
    let Some(me) = chat_ref(held, session) else {
        return;
    };
    if carried_on(held, session, &me) {
        return;
    }
    let Some(record) = dispatchrecord::running_for(held.root(), &me) else {
        return;
    };
    let ending = dispatchrecord::ended_unreported(&record, spent(held, session));
    // purlis's own word for a chat that went owing a report, said as the app's fact.
    let by = record
        .report_owed
        .then_some(dispatchrecord::EndedBy::Unreported);
    close(held, &record, ending, by, None);
    ended_in(held, session, &record);
}

/// Whether another open chat carries chat `session`, which is `me`, on: one started again in
/// its place is the same chat (its ULID, ADR 0066), under another number.
pub(crate) fn carried_on(held: &Held, session: u32, me: &ChatRef) -> bool {
    me.id.is_some()
        && held.chats().open_now().iter().any(|open| {
            open.session != session
                && held
                    .chats()
                    .chat_at(open.session)
                    .is_some_and(|other| other.id == me.id)
        })
}

/// The app wrote chat `session`'s session record at `path`: the dispatches it asked for are
/// listed on it, and the dispatch it worked on names it.
pub(crate) fn session_recorded(held: &Held, session: u32, path: &str) {
    if let Some(chat) = chat_ref(held, session) {
        dispatchrecord::session_recorded(held.root(), &chat, path);
    }
}

/// **The dispatch whose worktree is to be looked at once chat `session` is closed** (#1453):
/// the newest dispatch that chat worked on, where the app cut it a worktree that is still
/// kept. Asked before the chat is closed, while the app still knows it; `None` where a chat
/// started again in its place carries on, or another open chat stands in that folder.
pub(crate) fn worktree_to_look_at(held: &Held, session: u32) -> Option<String> {
    use purlis_core::dispatchplace::{self, Standing, Tree};

    held.chats().handed_from(session)?;
    let me = chat_ref(held, session)?;
    let record = dispatchrecord::latest_for(held.root(), &me)?;
    if dispatchplace::standing(held.root(), &record) != Some(Standing::Kept) {
        return None;
    }
    let tree = Tree::of(&record)?;
    let in_use = held.chats().open_now().iter().any(|open| {
        open.session != session
            && (open
                .cwd
                .as_deref()
                .is_some_and(|cwd| tree.holds(held.root(), cwd))
                || (me.id.is_some()
                    && held
                        .chats()
                        .chat_at(open.session)
                        .is_some_and(|other| other.id == me.id)))
    });
    (!in_use).then_some(record.id)
}

/// Chat `session` is closed, and dispatch `id` was what it worked on: where its worktree's
/// branch is merged, the worktree is taken away and its record says so
/// (`purlis_core::dispatchplace::tidy_recorded`). Anything else leaves it listed, with
/// Discard. Runs git, so the app calls it off the thread that closed the chat.
pub(crate) fn tidy_closed(root: &Path, id: &str) -> purlis_core::dispatchplace::Tidied {
    match dispatchrecord::read(root, id) {
        Some(record) => {
            purlis_core::dispatchplace::tidy_recorded(root, &record, &crate::gitbroker::isolation())
        }
        None => purlis_core::dispatchplace::Tidied::Kept,
    }
}

/// The project at `root` was opened, and `at_open` is the chats its reopen record brought
/// back as the open read it: the worktrees of its ended dispatches are looked at, once
/// (`purlis_core::dispatchplace::tidy_at_open`). Runs git only where a record still names a
/// worktree purlis has not taken away, so opening a project that has none costs a read of the
/// store and no more.
pub(crate) fn tidy_opened(root: &Path, at_open: &purlis_core::dispatchplace::AtOpen) {
    let any = dispatchrecord::list(root).iter().any(|record| {
        record
            .place
            .worktree
            .as_ref()
            .is_some_and(|tree| tree.removed.is_none())
    });
    if any {
        purlis_core::dispatchplace::tidy_at_open(root, at_open, &crate::gitbroker::isolation());
    }
}

fn running_for(held: &Held, session: u32) -> Option<Record> {
    dispatchrecord::running_for(held.root(), &chat_ref(held, session)?)
}

fn close(
    held: &Held,
    record: &Record,
    ending: Ending,
    by: Option<dispatchrecord::EndedBy>,
    way: Option<dispatchrecord::EndedWay>,
) {
    // **An answer of the person's the task never had** (#1496): where the app still holds
    // one for this task as its dispatch ends, no turn of the task was handed it. The record
    // says so on that answer, so the timeline does not read as if the task worked from it.
    let task = crate::activity::session_of(&record.worker.chat, &open_chats(held));
    if task.is_some_and(|task| held.tasks().ledger().talk.unread_answer(task)) {
        match dispatchrecord::answer_unread(held.root(), &record.id) {
            Ok(true) => crate::activity::answer_unread(held, &record.id),
            Ok(false) => {}
            Err(why) => tracing::warn!("purlis: a dispatch's record was not updated ({why})"),
        }
    }
    match dispatchrecord::close_as(held.root(), &record.id, ending, by, way, chrono::Utc::now()) {
        // An open Activity tab hears of the report it ended with (#1495).
        Ok(true) => {
            crate::activity::ended(held, &record.id);
            forget_where_its_asker_is_gone(held, record);
        }
        Ok(false) => {}
        Err(why) => tracing::warn!("purlis: a dispatch's record was not closed ({why})"),
    }
}

/// **A task that ends after the chat that asked for it has closed** (#1520): its row is
/// cleared and what the two said to each other is forgotten now, as they would have been had
/// it ended before that chat closed (`finished::asker_closed`). Nobody is left to see its row,
/// so nothing would clear it. A chat started again in the asking chat's place is the same chat
/// and still open, so it keeps its row. Not while the app is quitting, when every chat is
/// kept for the next launch.
fn forget_where_its_asker_is_gone(held: &Held, record: &Record) {
    if record.mode != dispatchrecord::Mode::Task
        || held.chats().ending()
        || crate::activity::session_of(&record.asker.chat, &open_chats(held)).is_some()
    {
        return;
    }
    if let Err(why) = dispatchrecord::clear_forgetting(held.root(), &record.id) {
        tracing::warn!("purlis: a task whose asking chat closed was not forgotten ({why})");
    }
}

/// What chat `session`'s harness says its conversation has cost, where it says.
fn spent(held: &Held, session: u32) -> Option<dispatchrecord::Usage> {
    purlis_core::usage::spent(held.root(), &conversation_of(held, session)?)
}

// ---------------------------------------------------------------------------------------
// The Dispatches tab
// ---------------------------------------------------------------------------------------

/// One dispatch, as the Dispatches tab draws its row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct DispatchRow {
    pub id: String,
    /// `task` or `handoff`.
    pub mode: String,
    /// The persona it went to; `null` for a chat started as none.
    pub persona: Option<String>,
    /// The task's name where the dispatch gave one, else the persona chat's name.
    pub task: String,
    /// The asking chat, by the name the person saw.
    pub asker: String,
    /// Which chat that is, for a filter: its id, or `#<number>` for one given none. Two chats
    /// that were called the same are still two.
    pub asker_key: String,
    pub asker_persona: Option<String>,
    /// Whether the person dispatched from that chat's tab themselves.
    pub by_person: bool,
    /// Where it worked: the workspace's name, or `project root`, and its worktree's branch.
    pub place: String,
    /// The folder it started in.
    pub folder: Option<String>,
    /// `running`; `done`, `blocked`, `failed`, `cancelled` or `stopped`, the report's own word;
    /// `handed off` for one that ended owing no report; or `not open` for one that has not
    /// ended and whose chat this app does not have open (it was not brought back, and runs
    /// again when it is).
    pub outcome: String,
    /// When it started, as the record keeps it (UTC, RFC 3339).
    pub started: String,
    /// When it ended, the same way; `null` while it runs.
    pub ended: Option<String>,
    /// How long it ran, or has run so far, spelled (`4m 30s`). Empty for one that is
    /// [`NOT_OPEN`]: no clock runs on a chat that is not there.
    pub duration: String,
    pub needed_you: u32,
    pub messages: u32,
    /// What its harness said it cost (`$0.42`); `null` where the harness reports none.
    pub cost: Option<String>,
    /// The tokens its harness counted, in and out (`15k in, 4k out`); `null` where it
    /// reports none.
    pub tokens: Option<String>,
    pub brief: String,
    /// The report's text, where it ended with one.
    pub report: Option<String>,
    /// What the report says changed, in the persona chat's words, where it said.
    pub changed: Option<String>,
    /// The persona chat's session, while it is still open: what the row opens.
    pub open_session: Option<u32>,
    /// Else its session record, by its project-relative path, once it wrote one.
    pub session_record: Option<String>,
    /// The worktree the app cut for it, where the dispatch gave it one (#1453), and how it
    /// stands. `null` for a dispatch that worked in a folder that was already there.
    pub worktree: Option<RowWorktree>,
}

/// A dispatch's worktree, as its row lists it (#1453).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct RowWorktree {
    /// The repo it was cut from.
    pub repo: String,
    /// The branch purlis cut for it.
    pub branch: Option<String>,
    /// `kept` while its folder is there; `merged` once purlis found its branch merged and took
    /// folder and branch away; `merged-branch-kept` where git kept the branch; `discarded` once the person discarded its folder;
    /// `gone` for one removed by other means.
    pub standing: String,
    /// Whether Discard is offered: its folder is there. Discard itself is refused while a chat
    /// still stands in it.
    pub discard: bool,
}

/// `record`'s worktree as its row lists it, in the project at `root`.
pub(crate) fn worktree_row(root: &Path, record: &Record) -> Option<RowWorktree> {
    use purlis_core::dispatchplace::{self, Standing};
    let tree = record.place.worktree.as_ref()?;
    let standing = dispatchplace::standing(root, record)?;
    Some(RowWorktree {
        repo: tree.repo.clone(),
        branch: tree.branch.clone(),
        standing: standing.word().to_owned(),
        discard: standing == Standing::Kept,
    })
}

/// What a row says of a dispatch that has not ended and whose chat is not open.
pub(crate) const NOT_OPEN: &str = "not open";

/// A chat the app has open, as far as a row needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenChat {
    pub session: u32,
    pub id: Option<String>,
}

/// `record` as its row, given the chats open now and the time.
pub(crate) fn row(
    record: &Record,
    open: &[OpenChat],
    now: chrono::DateTime<chrono::Utc>,
) -> DispatchRow {
    let worker = &record.worker.chat;
    // By the chat's id, which a restart keeps; by its number only for a record with no id.
    let open_session = open
        .iter()
        .find(|chat| dispatchrecord::named(worker, chat.id.as_deref(), Some(chat.session)))
        .map(|chat| chat.session);
    let outcome = match (&record.report, record.running()) {
        (Some(report), _) => report.outcome.word(),
        (None, true) if open_session.is_some() => "running",
        // Not ended, and its chat is not one this app has open: a chat the reopen record
        // lists that was not brought back. It is not running, and is not said to be (#1457).
        (None, true) => NOT_OPEN,
        (None, false) => "handed off",
    };
    let mut place = record
        .place
        .workspace
        .clone()
        .unwrap_or_else(|| "project root".to_owned());
    if let Some(branch) = record
        .place
        .worktree
        .as_ref()
        .and_then(|tree| tree.branch.as_deref())
    {
        place = format!("{place} · {branch}");
    }
    let usage = record.usage.as_ref();
    DispatchRow {
        id: record.id.clone(),
        mode: match record.mode {
            dispatchrecord::Mode::Task => "task",
            dispatchrecord::Mode::Handoff => "handoff",
        }
        .to_owned(),
        persona: record.persona.clone(),
        task: record.task.clone().unwrap_or_else(|| worker.name.clone()),
        asker: record.asker.chat.name.clone(),
        asker_key: record
            .asker
            .chat
            .id
            .clone()
            .unwrap_or_else(|| format!("#{}", record.asker.chat.chat)),
        asker_persona: record.asker.chat.persona.clone(),
        by_person: record.asker.by_person,
        place,
        folder: record.place.folder.clone(),
        outcome: outcome.to_owned(),
        started: record.started.clone(),
        ended: record.ended.clone(),
        duration: if outcome == NOT_OPEN {
            String::new()
        } else {
            lasted(record, now)
        },
        needed_you: record.needed_you,
        messages: record.messages,
        cost: usage.and_then(|usage| usage.cost_usd).map(dollars),
        tokens: usage.and_then(|usage| counted(usage.input_tokens, usage.output_tokens)),
        brief: record.brief.clone(),
        report: record.report.as_ref().map(|report| report.text.clone()),
        changed: record
            .report
            .as_ref()
            .and_then(|report| report.changed.said.clone()),
        open_session,
        session_record: record.worker.session_record.clone(),
        // Read from the project by the caller that lists rows ([`worktree_row`]).
        worktree: None,
    }
}

/// How long `record`'s dispatch ran, or has run by `now`: `45s`, `4m 30s`, `2h 5m`. Empty for
/// a record whose times do not read.
fn lasted(record: &Record, now: chrono::DateTime<chrono::Utc>) -> String {
    let at = |stamp: &str| {
        chrono::DateTime::parse_from_rfc3339(stamp)
            .ok()
            .map(|read| read.with_timezone(&chrono::Utc))
    };
    let Some(started) = at(&record.started) else {
        return String::new();
    };
    let ended = match record.ended.as_deref() {
        Some(ended) => match at(ended) {
            Some(ended) => ended,
            None => return String::new(),
        },
        None => now,
    };
    let seconds = (ended - started).num_seconds().max(0);
    let (hours, minutes, secs) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// A harness's cost figure as the window says it. A cost too small for two places is said as
/// under a cent, never as a zero the harness did not say.
fn dollars(cost: f64) -> String {
    if cost > 0.0 && cost < 0.005 {
        "under $0.01".to_owned()
    } else {
        format!("${cost:.2}")
    }
}

/// A harness's token counts as the window says them, or `None` where it counted neither.
pub(crate) fn counted(input: Option<u64>, output: Option<u64>) -> Option<String> {
    let spelled = |n: u64, way: &str| {
        format!(
            "{} {way}",
            purlis_core::usage::tokens(n.min(i64::MAX as u64) as i64)
        )
    };
    let parts: Vec<String> = input
        .map(|n| spelled(n, "in"))
        .into_iter()
        .chain(output.map(|n| spelled(n, "out")))
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// What the Dispatches tab is handed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct Dispatches {
    /// Newest first.
    pub rows: Vec<DispatchRow>,
    /// How many records in the store purlis will not draw (`dispatchrecord::sound`): text it
    /// refuses to put on the screen, or more of it than the store ever writes.
    pub undrawn: u32,
}

/// The chats `held` has open now, as a row needs them.
pub(crate) fn open_chats(held: &Held) -> Vec<OpenChat> {
    held.chats()
        .open_now()
        .into_iter()
        .map(|chat| OpenChat {
            session: chat.session,
            id: held.chats().chat_at(chat.session).and_then(|at| at.id),
        })
        .collect()
}

/// Every dispatch of `held`'s project purlis draws, newest first, and the count of those it
/// will not.
pub(crate) fn rows(held: &Held) -> Dispatches {
    let open = open_chats(held);
    let now = chrono::Utc::now();
    let drawn = dispatchrecord::drawn(held.root());
    Dispatches {
        rows: drawn
            .records
            .iter()
            .map(|record| DispatchRow {
                worktree: worktree_row(held.root(), record),
                ..row(record, &open, now)
            })
            .collect(),
        undrawn: u32::try_from(drawn.refused).unwrap_or(u32::MAX),
    }
}

/// Every dispatch of the project purlis draws, newest first, for its Dispatches tab (#1452):
/// the running ones and the past ones this machine still keeps. On a blocking thread, as it reads every
/// record.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dispatches(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
) -> Result<Dispatches, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading the project's dispatches", move || Ok(rows(&held))).await
}

// ---------------------------------------------------------------------------------------
// Discard
// ---------------------------------------------------------------------------------------

/// What discarding the folder of a dispatch's own branch would take with it, as the window
/// shows it before it asks (#1453), and as the window hands it back with the answer: **the
/// paths discarded are the ones the person was shown, or nothing is.**
///
/// A comparison of paths, and it says so: every uncommitted file is listed by its own path,
/// so a new one is seen. A listed file changed again, or a file added inside a folder git
/// ignores whole, is the same list and passes. The moment between the last read and git's
/// removal is not covered either.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub(crate) struct WorktreeLoss {
    /// The task, by the name its row has.
    pub task: String,
    /// The repo the branch is in.
    pub repo: String,
    /// The branch purlis cut for the task, by the dispatch's record: the one a discard may
    /// delete, and only where git finds it merged.
    pub branch: Option<String>,
    /// The branch the folder is on now; `null` for a folder on no branch (a detached HEAD).
    /// The record's, unless the chat switched its folder away from it.
    pub on: Option<String>,
    /// Every uncommitted path, as git prints it (`?? new.txt`, ` M a.rs`): lost with the
    /// folder.
    pub changes: Vec<String>,
    /// Every path git ignores there, a folder as one entry (`target/`): build output, local
    /// settings, and what purlis keeps hidden in a chat's folder. Deleted with the folder.
    pub ignored: Vec<String>,
    /// How many commits the folder holds that exist on no other branch and no remote. **Where
    /// the folder is on a branch they are not lost**: that branch stays. Where it is on none,
    /// nothing keeps them and they go with the folder.
    pub unmerged: u32,
    /// Those commits, newest first, as `<short sha> <subject>` and at most eleven of them,
    /// **where they would be lost**: the folder is on no branch. Empty otherwise.
    pub lost: Vec<String>,
}

/// What a discard is told of a folder that is no longer there.
const ALREADY_GONE: &str = "That branch's folder is already gone, so there is nothing to discard.";

/// Why a branch's folder is not discarded while a chat stands in it.
fn still_open(task: &str) -> String {
    format!(
        "A chat is still open in the folder of the branch cut for '{task}'. Close it first: \
         purlis will not remove a folder a chat is working in."
    )
}

/// What stands in the way of discarding the worktree of `record`, where something does: its
/// dispatch still runs, its chat is open, or any other open chat stands in its folder.
fn in_the_way(
    held: &Held,
    record: &Record,
    tree: &purlis_core::dispatchplace::Tree,
) -> Option<String> {
    let task = record
        .task
        .clone()
        .unwrap_or_else(|| record.worker.chat.name.clone());
    chat_in_the_way(held, record, tree).then(|| still_open(&task))
}

/// Whether the task of `record` still runs, its chat is open, or any other open chat stands in
/// the folder of its own branch: what a discard and a merge (#1511) are both refused over.
pub(crate) fn chat_in_the_way(
    held: &Held,
    record: &Record,
    tree: &purlis_core::dispatchplace::Tree,
) -> bool {
    let open = held.chats().open_now();
    let its_chat = open.iter().any(|chat| {
        let id = held.chats().chat_at(chat.session).and_then(|at| at.id);
        dispatchrecord::named(&record.worker.chat, id.as_deref(), Some(chat.session))
    });
    let stood_in = open.iter().any(|chat| {
        chat.cwd
            .as_deref()
            .is_some_and(|cwd| tree.holds(held.root(), cwd))
    });
    record.running() || its_chat || stood_in
}

/// The dispatch `id` and its worktree, where it has one that is still kept and no chat stands
/// in the way.
fn discardable(
    held: &Held,
    id: &str,
) -> Result<(Record, purlis_core::dispatchplace::Tree), String> {
    use purlis_core::dispatchplace::{self, Standing, Tree};
    let record = dispatchrecord::read(held.root(), id)
        .ok_or_else(|| "purlis has no record of that dispatch.".to_owned())?;
    if dispatchplace::standing(held.root(), &record) != Some(Standing::Kept) {
        return Err(ALREADY_GONE.to_owned());
    }
    let tree = Tree::of(&record).ok_or_else(|| ALREADY_GONE.to_owned())?;
    if let Some(why) = in_the_way(held, &record, &tree) {
        return Err(why);
    }
    Ok((record, tree))
}

/// What discarding the folder of dispatch `id`'s branch would take with it, read now.
///
/// Refused where git will not say what is there: a confirmation that cannot name what would
/// be lost is not one purlis asks for.
pub(crate) fn loss_of(held: &Held, id: &str) -> Result<WorktreeLoss, String> {
    let (record, tree) = discardable(held, id)?;
    let risk =
        purlis_core::dispatchplace::at_risk(held.root(), &tree, &crate::gitbroker::isolation())
            .map_err(|not_done| not_done.in_window(&tree.repo))?
            .ok_or_else(|| ALREADY_GONE.to_owned())?;
    let unread = || {
        "purlis could not read what that branch's folder holds, so it will not discard it: it \
         could not tell you what would be lost. Look in the folder by hand."
            .to_owned()
    };
    Ok(WorktreeLoss {
        task: record
            .task
            .clone()
            .unwrap_or_else(|| record.worker.chat.name.clone()),
        repo: tree.repo.clone(),
        // The record's branch, never whatever the folder is on now: it is the only branch a
        // discard touches.
        branch: tree.branch.clone(),
        changes: risk.changes.ok_or_else(unread)?,
        ignored: risk.ignored.ok_or_else(unread)?,
        unmerged: risk.unmerged.ok_or_else(unread)?,
        // On no branch, nothing keeps the folder's own commits: they are named, as lost.
        lost: if risk.branch.is_none() {
            risk.commits
        } else {
            Vec::new()
        },
        on: risk.branch,
    })
}

/// **Discards the folder of dispatch `id`'s own branch**, whatever is in it, where that is
/// exactly what `seen` says the person was shown. Its branch goes too only where git finds it
/// merged; one that holds a commit stays (`purlis_core::dispatchplace::discard`).
///
/// Read again at this moment and compared: a file written or a commit made since the question
/// was asked is something the person did not agree to, so nothing is removed and they are
/// asked again. And refused, as the question was, while any chat stands in the folder.
pub(crate) fn discard(held: &Held, id: &str, seen: &WorktreeLoss) -> Result<(), String> {
    let now = loss_of(held, id)?;
    if now != *seen {
        return Err(
            "What that branch's folder holds has changed since you were asked, so nothing was \
             removed. Press Discard again to see what would be lost now."
                .to_owned(),
        );
    }
    // Asked once more, last: a chat started there while git was read stands in the way too.
    let (record, tree) = discardable(held, id)?;
    purlis_core::dispatchplace::discard(held.root(), &tree, &crate::gitbroker::isolation())
        .map_err(|not_done| not_done.in_window(&tree.repo))?;
    if let Err(why) = dispatchrecord::worktree_removed(
        held.root(),
        &record.id,
        dispatchrecord::Removed::Discarded,
    ) {
        tracing::warn!("purlis: a discarded worktree's record was not updated ({why})");
    }
    Ok(())
}

/// What discarding the folder of dispatch `id`'s own branch would take with it (#1453): every
/// uncommitted file, every ignored one, and how many commits its branch holds that exist
/// nowhere else, for the question the window asks before it discards. Refused while a chat is
/// still open in the folder, and where git will not say what is there.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dispatch_worktree_loss(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    id: String,
) -> Result<WorktreeLoss, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading what the branch's folder holds", move || {
        loss_of(&held, &id)
    })
    .await
}

/// **Discard** on a dispatch's own branch (#1453): removes its folder, for good, with every
/// uncommitted and ignored file in it. The branch is kept unless git finds it merged, so no
/// commit is lost. `seen` is what the window showed the person would go, as
/// `dispatch_worktree_loss` answered it: where the folder holds anything else by now, nothing
/// is removed. Refused while a chat is still open in the folder. Nothing is merged.
#[tauri::command]
#[specta::specta]
pub(crate) async fn dispatch_worktree_discard(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    id: String,
    seen: WorktreeLoss,
) -> Result<(), String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("discarding the branch's folder", move || {
        discard(&held, &id, &seen)
    })
    .await
}

/// One dispatch a chat made, as its session record's tab lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct DispatchMade {
    pub persona: Option<String>,
    pub task: String,
    /// As a row's ([`DispatchRow::outcome`]).
    pub outcome: String,
}

/// The dispatches listed on the session record at `path`, in the order they were made. `open`
/// is the chats the app has open now ([`open_chats`]), which is what says a dispatch that has
/// not ended is running.
pub(crate) fn made_on(root: &Path, path: &str, open: &[OpenChat]) -> Vec<DispatchMade> {
    let now = chrono::Utc::now();
    dispatchrecord::listed_on(root, path)
        .iter()
        .map(|record| {
            let row = row(record, open, now);
            DispatchMade {
                persona: row.persona,
                task: row.task,
                outcome: row.outcome,
            }
        })
        .collect()
}

/// [`made_on`] for the project `held` holds: what a session record's tab lists, with the
/// chats open now.
pub(crate) fn made_by(held: &Held, path: &str) -> Vec<DispatchMade> {
    made_on(held.root(), path, &open_chats(held))
}

#[cfg(test)]
mod tests {
    use purlis_core::dispatchrecord::{
        Asker, Changed, Mode, Place, Report, Usage, Worker, Worktree,
    };

    use super::*;

    fn at(time: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(time)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    fn a_record() -> Record {
        Record {
            v: dispatchrecord::VERSION,
            id: "01K6DISPATCH00000000000000".to_owned(),
            mode: Mode::Handoff,
            asker: Asker {
                chat: ChatRef {
                    chat: 3,
                    id: Some("asker".to_owned()),
                    name: "steward 3".to_owned(),
                    persona: Some("steward".to_owned()),
                },
                workspace: Some("alpha".to_owned()),
                by_person: false,
                session_record: None,
            },
            persona: Some("devops".to_owned()),
            worker: Worker {
                chat: ChatRef {
                    chat: 7,
                    id: Some("worker".to_owned()),
                    name: "devops 7".to_owned(),
                    persona: Some("devops".to_owned()),
                },
                harness: Some("claude".to_owned()),
                profile: Some("work".to_owned()),
                session_record: None,
            },
            task: Some("check prod".to_owned()),
            place: Place {
                workspace: Some("beta".to_owned()),
                folder: Some("workspaces/beta".to_owned()),
                worktree: None,
            },
            brief: "Is the rollout healthy?".to_owned(),
            report_owed: true,
            started: "2026-10-07T12:00:00+00:00".to_owned(),
            ended: None,
            report: None,
            needed_you: 2,
            messages: 0,
            talk: Vec::new(),
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
            undelivered: None,
            asker_last_record: None,
        }
    }

    #[test]
    fn a_running_dispatch_s_row_opens_its_chat_and_counts_its_time_so_far() {
        // The chat was started again since: another session, the same chat.
        let open = [OpenChat {
            session: 12,
            id: Some("worker".to_owned()),
        }];

        let row = row(&a_record(), &open, at("2026-10-07T12:04:30Z"));

        assert_eq!(row.outcome, "running");
        assert_eq!(row.duration, "4m 30s");
        assert_eq!(row.open_session, Some(12));
        assert_eq!(row.asker_key, "asker");
        assert_eq!(row.mode, "handoff");
        assert_eq!(row.ended, None);
        assert_eq!(row.persona.as_deref(), Some("devops"));
        assert_eq!(row.task, "check prod");
        assert_eq!(row.asker, "steward 3");
        assert_eq!(row.place, "beta");
        assert_eq!(row.needed_you, 2);
        assert_eq!(row.cost, None);
        assert_eq!(row.tokens, None);
    }

    #[test]
    fn a_finished_dispatch_s_row_says_its_outcome_its_cost_and_its_session_record() {
        let record = Record {
            ended: Some("2026-10-07T14:05:00+00:00".to_owned()),
            report: Some(Report {
                outcome: Outcome::Blocked,
                text: "No access to the cluster.".to_owned(),
                changed: Changed::default(),
            }),
            usage: Some(Usage {
                input_tokens: Some(15_234),
                output_tokens: Some(4_521),
                cost_usd: Some(0.416),
            }),
            worker: Worker {
                session_record: Some("workspaces/beta/sessions/x.md".to_owned()),
                ..a_record().worker
            },
            place: Place {
                worktree: Some(Worktree {
                    repo: "svc".to_owned(),
                    piece: "fix".to_owned(),
                    branch: Some("fix/rollout".to_owned()),
                    removed: None,
                }),
                ..a_record().place
            },
            ..a_record()
        };

        // Its chat is closed: nothing open is it, whatever number another chat has.
        let row = row(
            &record,
            &[OpenChat {
                session: 7,
                id: Some("someone else".to_owned()),
            }],
            at("2026-10-09T00:00:00Z"),
        );

        assert_eq!(row.outcome, "blocked");
        assert_eq!(row.duration, "2h 5m");
        assert_eq!(row.cost.as_deref(), Some("$0.42"));
        assert_eq!(row.tokens.as_deref(), Some("15k in, 4k out"));
        assert_eq!(row.report.as_deref(), Some("No access to the cluster."));
        assert_eq!(row.open_session, None);
        assert_eq!(
            row.session_record.as_deref(),
            Some("workspaces/beta/sessions/x.md")
        );
        assert_eq!(row.place, "beta · fix/rollout");
    }

    /// A number is dealt again in another launch: a row never opens a chat because it has
    /// the number the dispatch's chat once had.
    #[test]
    fn a_row_never_opens_another_chat_that_was_dealt_its_chat_s_number() {
        let namesakes = [
            OpenChat {
                session: 7,
                id: Some("someone else".to_owned()),
            },
            OpenChat {
                session: 7,
                id: None,
            },
        ];

        let row = row(&a_record(), &namesakes, at("2026-10-07T12:04:30Z"));

        assert_eq!(row.open_session, None);
        // Only a record that names its chat by nothing but a number is matched by one.
        let numbered = Record {
            worker: Worker {
                chat: ChatRef {
                    id: None,
                    ..a_record().worker.chat
                },
                ..a_record().worker
            },
            asker: Asker {
                chat: ChatRef {
                    id: None,
                    ..a_record().asker.chat
                },
                ..a_record().asker
            },
            ..a_record()
        };
        let row = super::row(&numbered, &namesakes, at("2026-10-07T12:04:30Z"));
        assert_eq!(row.open_session, Some(7));
        assert_eq!(row.asker_key, "#3");
    }

    /// #1457: a record whose chat the reopen record lists but which was not brought back
    /// showed "running", with a duration that grew, until the project was next opened.
    #[test]
    fn a_dispatch_that_has_not_ended_and_whose_chat_is_not_open_is_not_said_to_be_running() {
        let row = row(&a_record(), &[], at("2026-10-07T12:04:30Z"));

        assert_eq!(row.outcome, NOT_OPEN);
        assert_eq!(row.outcome, "not open");
        assert_eq!(
            row.duration, "",
            "no clock runs on a chat that is not there"
        );
        assert_eq!(row.ended, None);
        assert_eq!(row.open_session, None);
        // One that ended says how, open or not.
        let ended = Record {
            ended: Some("2026-10-07T12:01:00+00:00".to_owned()),
            report: Some(Report {
                outcome: Outcome::Stopped,
                text: "stopped by the operator".to_owned(),
                changed: Changed::default(),
            }),
            ..a_record()
        };
        let row = super::row(&ended, &[], at("2026-10-09T00:00:00Z"));
        assert_eq!(
            (row.outcome.as_str(), row.duration.as_str()),
            ("stopped", "1m 0s")
        );
        assert_eq!(row.changed, None);
    }

    #[test]
    fn a_row_says_what_the_report_says_changed_in_the_chat_s_words() {
        let record = Record {
            ended: Some("2026-10-07T12:01:00+00:00".to_owned()),
            report: Some(Report {
                outcome: Outcome::Cancelled,
                text: "Stopped half way.".to_owned(),
                changed: Changed {
                    said: Some("svc: 2 files".to_owned()),
                    ..Changed::default()
                },
            }),
            ..a_record()
        };

        let row = row(&record, &[], at("2026-10-09T00:00:00Z"));

        assert_eq!(row.outcome, "cancelled");
        assert_eq!(row.changed.as_deref(), Some("svc: 2 files"));
    }

    #[test]
    fn a_handoff_that_ended_owing_no_report_is_handed_off_and_has_no_cost_to_say() {
        let record = Record {
            report_owed: false,
            ended: Some("2026-10-07T12:00:45+00:00".to_owned()),
            task: None,
            place: Place::default(),
            ..a_record()
        };

        let row = row(&record, &[], at("2026-10-09T00:00:00Z"));

        assert_eq!(row.outcome, "handed off");
        assert_eq!(row.duration, "45s");
        // Named for its chat where the dispatch gave no task name.
        assert_eq!(row.task, "devops 7");
        assert_eq!(row.place, "project root");
        assert_eq!(row.cost, None);
    }

    #[test]
    fn a_cost_is_said_as_the_harness_said_it_and_a_zero_only_when_it_said_zero() {
        assert_eq!(dollars(0.0), "$0.00");
        assert_eq!(dollars(0.001), "under $0.01");
        assert_eq!(dollars(12.345), "$12.35");
        assert_eq!(counted(None, None), None);
        assert_eq!(counted(Some(900), None).as_deref(), Some("900 in"));
    }

    #[test]
    fn a_wait_is_counted_when_it_begins_and_not_at_every_report_made_during_it() {
        let waiting = Waiting::default();

        assert!(waiting.began(7, true));
        assert!(!waiting.began(7, true));
        assert!(!waiting.began(7, false));
        assert!(waiting.began(7, true));
        // A chat closed while waiting starts from nothing.
        waiting.forget(7);
        assert!(waiting.began(7, true));
    }

    #[test]
    fn a_row_lists_the_worktree_its_dispatch_was_given_and_offers_discard_only_while_it_is_kept() {
        use purlis_core::dispatchrecord::Removed;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let given = |piece: &str, removed: Option<Removed>| Record {
            ended: Some("2026-10-07T14:05:00+00:00".to_owned()),
            place: Place {
                workspace: Some("alpha".to_owned()),
                folder: Some(format!("workspaces/alpha/.worktrees/svc/{piece}")),
                worktree: Some(Worktree {
                    repo: "svc".to_owned(),
                    piece: piece.to_owned(),
                    branch: Some(piece.to_owned()),
                    removed,
                }),
            },
            ..a_record()
        };
        let listed = |record: &Record| {
            worktree_row(root, record).map(|listed| (listed.standing, listed.discard))
        };
        std::fs::create_dir_all(root.join("workspaces/alpha/.worktrees/svc/fix-00000000")).unwrap();

        // Its folder is there and nothing has merged it: listed, with Discard.
        let kept = given("fix-00000000", None);
        assert_eq!(listed(&kept), Some(("kept".to_owned(), true)));
        let row = worktree_row(root, &kept).unwrap();
        assert_eq!(
            (row.repo.as_str(), row.branch.as_deref()),
            ("svc", Some("fix-00000000"))
        );
        // What purlis did to it is the record's word, and neither is discarded twice.
        assert_eq!(
            listed(&given("fix-00000000", Some(Removed::Merged))),
            Some(("merged".to_owned(), false))
        );
        assert_eq!(
            listed(&given("fix-00000000", Some(Removed::MergedBranchKept))),
            Some(("merged-branch-kept".to_owned(), false))
        );
        assert_eq!(
            listed(&given("fix-00000000", Some(Removed::Discarded))),
            Some(("discarded".to_owned(), false))
        );
        // A record that names a branch folder purlis did not cut for it offers no Discard: its
        // name does not end with the end of the record's own id.
        std::fs::create_dir_all(root.join("workspaces/alpha/.worktrees/svc/chat-1")).unwrap();
        assert_eq!(
            listed(&given("chat-1", None)),
            Some(("gone".to_owned(), false))
        );
        // Its folder gone by other hands, and a record whose names climb out of the project.
        assert_eq!(
            listed(&given("other-00000000", None)),
            Some(("gone".to_owned(), false))
        );
        assert_eq!(
            listed(&given("../../../etc", None)),
            Some(("gone".to_owned(), false))
        );
        // A dispatch that worked in a folder that was already there lists none.
        assert_eq!(listed(&a_record()), None);
        // And the row's place names the branch, as it did.
        assert_eq!(
            row_of(&kept).place,
            "alpha · fix-00000000",
            "the workspace, then its own branch"
        );
    }

    /// `record`'s row with nothing open, at a fixed time.
    fn row_of(record: &Record) -> DispatchRow {
        row(record, &[], at("2026-10-09T00:00:00Z"))
    }

    #[test]
    fn a_folder_is_said_from_the_project_and_one_outside_it_whole() {
        let root = Path::new("/p/project");
        assert_eq!(folder(root, Path::new("/p/project")), ".");
        assert_eq!(
            folder(root, Path::new("/p/project/workspaces/beta")),
            "workspaces/beta"
        );
        assert_eq!(folder(root, Path::new("/elsewhere/x")), "/elsewhere/x");
    }
}
