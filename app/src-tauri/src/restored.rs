//! The app's half of tasks across a restart, and of a task whose asking chat has gone (#1513,
//! V100-63, V100-64). The rules are `purlis_core::dispatchrestart`'s; this module feeds them
//! what only the app has (which chats it opened, which wait to start, which it is about to
//! start) and does what they answer.
//!
//! # At a launch
//!
//! [`put_back`] reads the dispatch store once and says, of the record of open chats, which
//! tasks still owing a report are told to carry on as they start, and which cannot be told so
//! and are left out: those have ended by itself, and the chat that asked is told, once.
//!
//! **A refused start is not an end.** A task whose start the launch refused (every agent was
//! stopped, a profile wants approving again, a folder moved) waits in the list of chats that
//! did not start, its dispatch still running, drawn under the chat that asked (#1497). **Try
//! to start again** on that row starts it told to carry on, while that dispatch still runs
//! ([`retrying`]); **End task** on it ends it and tells the chat that asked
//! (`crate::unstarted::end`). Forget, for a chat listed across the window, does the same
//! ([`forgetting`]).
//!
//! A dispatch whose chat neither came back nor waits to has ended, **where the record was read
//! or the person chose to start fresh**: a record that could not be read says nothing about
//! which chats are gone, and ends nothing.
//!
//! **Nothing is dispatched a second time**: a task comes back on its conversation, told one
//! fixed sentence, never its brief; and a brief its asking chat sends again after its run
//! began anew, while the task it started before is still at work, is refused
//! ([`refuse_twice`]).
//!
//! # A report whose asking chat has gone
//!
//! A task's report kept for its workspace because the chat that asked had gone, or left for
//! that chat and unread when it closed, is marked on the task's dispatch record
//! ([`kept_for_its_asker`], [`unread_at_close`]). When that chat comes back (a launch, Retry
//! now on a chat that did not start, Resume from one of its session records, Reopen on its
//! finished row where it was a task), every such report is taken back from the workspace
//! before the chat starts and handed to it once it has: **its record says it is owed until
//! then**, so a start the app dies in leaves it owed and not lost. **A report that lands after the chat came back** goes to it at once: the chat that
//! resumed one of its session records knows whose they were
//! (`purlis_core::reopen::Identity::resumed_from`).
//!
//! Every hand-over runs under `Chats::deciding()`, the lock a report is delivered under, so a
//! report that lands while an asking chat starts goes either to it or onto its record, and is
//! taken from there.

use std::path::Path;

use purlis_core::dispatchrecord::{self, ChatRef};
use purlis_core::dispatchrestart::{self, AtLaunch, NotBack, Owing};
use purlis_core::reopen::{Chat, Record};

use crate::planes::Held;

/// **Puts back `record` at a launch** (#1513), as `Chats::put_back_telling` does, with what is
/// said above: the chats that came back. `settles` says the record was read, or the person
/// chose to start fresh: only then is a dispatch nothing brings back ended.
pub(crate) fn put_back(
    held: &Held,
    record: &Record,
    size: purlis_core::engine::Size,
    settles: bool,
) -> Vec<crate::chats::Open> {
    let launch =
        dispatchrestart::at_launch(held.root(), record, &dispatchrecord::list(held.root()));
    for chat in &launch.not_resumed {
        tracing::info!(
            "purlis: task chat '{}' could not be told to carry on; it has ended, and the chat \
             that asked is told",
            chat.label.as_deref().unwrap_or(&chat.name)
        );
    }
    let coming = coming_back(held, &launch.back);
    let opened = held
        .chats()
        .put_back_telling(&launch.back, size, &|chat| launch.told(chat));
    for chat in &launch.back.chats {
        if launch.told(chat).is_some()
            && let Some(number) = chat.number
        {
            questions_were_not_kept(held, number);
        }
    }
    after_put_back(held, &launch, coming, settles);
    opened
}

/// Task chat `task` was told to carry on (#1546): what it asked its asking chat before the
/// restart was in the app's memory and is gone, and an answer to it is told so.
fn questions_were_not_kept(held: &Held, task: u32) {
    held.tasks().ledger().talk.restored(task);
}

/// The reports kept for chats `record` brings back, taken from their workspaces before any of
/// those chats starts: handed to each once it has ([`after_put_back`]).
fn coming_back(held: &Held, record: &Record) -> AskerComing {
    let chats: Vec<Known> = record.chats.iter().map(Known::of).collect();
    AskerComing::take(held, move |dispatch| {
        chats.iter().any(|chat| chat.asked(dispatch))
    })
}

/// A chat as the reports kept for it are matched to it: its id, and the chat it resumed.
struct Known {
    id: Option<String>,
    resumed_from: Option<String>,
}

impl Known {
    fn of(chat: &Chat) -> Self {
        Self {
            id: chat.identity.id.clone(),
            resumed_from: chat.identity.resumed_from.clone(),
        }
    }

    /// Whether `dispatch` was asked for by this chat, or by the chat it resumed.
    fn asked(&self, dispatch: &dispatchrecord::Record) -> bool {
        let asker = dispatch.asker.chat.id.as_ref();
        asker.is_some() && (asker == self.id.as_ref() || asker == self.resumed_from.as_ref())
    }
}

/// **After the launch's put-back** (#1513): what could not be told to carry on has ended by
/// itself and its asking chat is told; where `settles`, what neither came back nor waits has
/// ended; the reports kept for chats that came back are handed to them.
fn after_put_back(held: &Held, launch: &AtLaunch, coming: AskerComing, settles: bool) {
    let root = held.root();
    let now = chrono::Utc::now();
    let deciding = held.chats().deciding();
    let mut told = Vec::new();
    for chat in &launch.not_resumed {
        let why = if chat.resume.is_none() {
            NotBack::NoConversation
        } else {
            NotBack::NoProfile
        };
        told.extend(end_it(held, chat, why, now));
    }
    if settles {
        let waiting: Vec<String> = held
            .chats()
            .would_not_start()
            .into_iter()
            .map(|one| one.id)
            .collect();
        let live = |worker: &ChatRef| {
            worker
                .id
                .as_ref()
                .is_some_and(|id| waiting.contains(id) || held.chats().id_is_open(id))
        };
        dispatchrestart::settle_after_launch(root, live, now);
    }
    // What was kept for a chat that is open now is handed to it: taken before the put-back,
    // and what a settle above kept for one.
    let settled = AskerComing::take_under(held, |dispatch| asked_by_an_open_chat(held, dispatch));
    coming.back_to_open(held);
    settled.back_to_open(held);
    drop(deciding);
    tell_the_ledger(held, told);
    held.rows_changed();
}

/// Whether an open chat asked for `dispatch`, or resumed the chat that did.
fn asked_by_an_open_chat(held: &Held, dispatch: &dispatchrecord::Record) -> bool {
    held.chats()
        .open_now()
        .iter()
        .filter_map(|open| held.chats().recorded_chat(open.session))
        .any(|chat| Known::of(&chat).asked(dispatch))
}

/// [`dispatchrestart::end_at_launch`] for `chat`, with what to tell the ledger where it told
/// the chat that asked. The open chat under the number the entry names is passed by its id,
/// and the core tells it only where it is the chat the dispatch was asked by.
fn end_it(
    held: &Held,
    chat: &Chat,
    why: NotBack,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<(u32, dispatchrestart::Ended)> {
    let asker = chat
        .from
        .as_ref()
        .and_then(|from| held.chats().chat_at(from.chat))
        .and_then(|at| at.id);
    match dispatchrestart::end_at_launch(held.root(), chat, asker.as_deref(), why, now) {
        Ok(Some(ended)) if ended.file.is_some() => Some((ended.asker, ended)),
        Ok(_) => None,
        Err(why) => {
            tracing::warn!("purlis: a task that did not come back was not settled ({why})");
            None
        }
    }
}

/// What waits in an asking chat's folder is a closed task's report: a wait on it reads it, and
/// the asking chat is told a report landed, as for any closed task. Never under the lock.
fn tell_the_ledger(held: &Held, told: Vec<(u32, dispatchrestart::Ended)>) {
    for (asker, ended) in told {
        if let Some(task) = ended.task {
            let mut ledger = held.tasks().ledger();
            ledger.reported(task, asker, ended.report, ended.file);
            ledger.forget(task, Some((asker, &ended.name)));
        }
        crate::dispatched::told_or_settled(held, asker);
    }
}

/// **Forget on a chat a launch could not start** (#1513): `forget` lets it go, and where it was
/// a task still owing its report, it has ended by itself and the chat that asked is told,
/// once. Anything else is forgotten as it always was.
pub(crate) fn forgetting(
    held: &Held,
    id: &str,
    forget: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let chat = held.chats().waiting_chat(id);
    forget()?;
    let Some(chat) = chat else {
        return Ok(());
    };
    let told = {
        let _deciding = held.chats().deciding();
        end_it(held, &chat, NotBack::LetGo, chrono::Utc::now())
    };
    tell_the_ledger(held, told.into_iter().collect());
    held.rows_changed();
    Ok(())
}

/// **A brief its asking chat already dispatched before its current run is refused** (#1513):
/// the chat `asker`, whose run began after it dispatched the task (purlis restarted it, or it
/// was started again or cleared), sends `brief` to `persona` again while that task still works.
/// A chat in the run that dispatched it is asking for a second task on purpose, and gets one.
pub(crate) fn refuse_twice(
    held: &Held,
    asker: u32,
    asking: &Chat,
    persona: Option<&str>,
    brief: &str,
) -> Result<(), String> {
    let (Some(id), Some(run)) = (
        asking.identity.id.as_deref(),
        asking.identity.run.as_deref(),
    ) else {
        return Ok(());
    };
    let me = ChatRef {
        chat: asker,
        id: Some(id.to_owned()),
        ..ChatRef::default()
    };
    let restored =
        dispatchrestart::made_before_its_run(dispatchrecord::list(held.root()), &me, run);
    let Some(before) = dispatchrestart::dispatched_before(&restored, persona, brief) else {
        return Ok(());
    };
    // By its number now: a task brought back keeps its id, and its number is its own.
    let number = held.chats().open_now().into_iter().find(|one| {
        held.chats().chat_at(one.session).and_then(|at| at.id) == before.worker.chat.id
    });
    match number {
        Some(open) => Err(dispatchrestart::not_again(
            before.task.as_deref().unwrap_or(&before.worker.chat.name),
            open.session,
        )),
        // Its chat is not open: nothing of it is at work, and a new task is a new task.
        None => Ok(()),
    }
}

/// **The session record chat `chat` wrote**, by its project-relative path: the one the app noted
/// in this launch, else the newest its dispatch record names (#1456, #1513). The app's note is
/// kept in memory and is gone after a relaunch; the record is not.
pub(crate) fn its_session_record(held: &Held, chat: u32) -> Option<String> {
    held.chats().last_record(chat).or_else(|| {
        let me = crate::dispatches::chat_ref(held, chat)?;
        dispatchrecord::latest_for(held.root(), &me)?
            .worker
            .session_record
    })
}

/// **A task's report was kept for its workspace, at `file`, because the chat that asked had
/// gone** (#1513): its dispatch record says so, and names the file, so that chat is handed it
/// if the person reopens it ([`to_a_resumed_asker`] hands it at once to one already reopened).
/// `chat` is the task's chat, still open. Under the lock a report is delivered under.
pub(crate) fn kept_for_its_asker(held: &Held, chat: u32, file: &Path) {
    let Some(me) = crate::dispatches::chat_ref(held, chat) else {
        return;
    };
    let Some(record) = dispatchrecord::latest_for(held.root(), &me) else {
        return;
    };
    let kept = purlis_core::handback::kept_name(held.root(), file);
    if let Err(why) = dispatchrecord::kept_undelivered(held.root(), &record.id, kept.as_deref()) {
        tracing::warn!(
            "purlis: a report kept for a workspace was not marked for its asker ({why})"
        );
    }
}

/// **Task chat `chat`'s dispatch has just ended, and its report reached no chat** (#1513):
/// where an open chat resumed the chat that asked (`Identity::resumed_from`), the report is
/// handed to that chat now, once, and taken back from the workspace. Called as the record is
/// closed, under the lock a report is delivered under: only an ended record is handed over.
pub(crate) fn to_a_resumed_asker(held: &Held, chat: u32) {
    let Some(me) = crate::dispatches::chat_ref(held, chat) else {
        return;
    };
    let Some(record) = dispatchrecord::latest_for(held.root(), &me) else {
        return;
    };
    let (Some(_), Some(asker)) = (record.undelivered.as_ref(), record.asker.chat.id.as_deref())
    else {
        return;
    };
    let resumed = held.chats().open_now().into_iter().find(|open| {
        held.chats()
            .recorded_chat(open.session)
            .is_some_and(|one| one.identity.resumed_from.as_deref() == Some(asker))
    });
    if let Some(resumed) = resumed {
        let owing = dispatchrestart::take_back(held.root(), |one| one.id == record.id);
        dispatchrestart::hand_to(held.root(), &owing, resumed.session);
    }
}

/// **Chat `session` is closing** (#1513): `asker` is it, as a record names it, read while it
/// was open. The task reports that waited for its next turn were moved to its workspace
/// (`moved`), and their records say so. A chat started again in its place carries them on, so
/// nothing is marked for it.
pub(crate) fn unread_at_close(
    held: &Held,
    session: u32,
    asker: Option<&ChatRef>,
    moved: &[(purlis_core::handback::Handback, Option<std::path::PathBuf>)],
) {
    let Some(asker) = asker else {
        return;
    };
    if moved.is_empty() || crate::dispatches::carried_on(held, session, asker) {
        return;
    }
    dispatchrestart::unread_at_close(held.root(), asker, moved);
}

/// **Resume from the session record at `path`** (#1513, V100-64): `chat` is the chat that
/// resumes it, which `start` starts.
///
/// The chat that wrote the record is known by the dispatches it asked for, which name the
/// record as its first or its newest since each (`dispatchrecord::Record::asker_last_record`),
/// and the new chat is started as having resumed it (`Identity::resumed_from`): a report of
/// one of its tasks that lands later reaches the new chat ([`kept_for_its_asker`]). The reports
/// kept for it already are taken from the workspace before the start and left for the new
/// chat's next turn once it has started. What `start` answers is answered.
pub(crate) fn resuming(
    held: &Held,
    path: &str,
    mut chat: Chat,
    start: impl FnOnce(&Chat) -> Result<u32, String>,
) -> Result<u32, String> {
    let named = |record: &dispatchrecord::Record| {
        record.asker.session_record.as_deref() == Some(path)
            || record.asker_last_record.as_deref() == Some(path)
    };
    let wrote = dispatchrecord::list(held.root())
        .into_iter()
        .find(|record| named(record) && record.asker.chat.id.is_some())
        .and_then(|record| record.asker.chat.id);
    if chat.identity.resumed_from.is_none() {
        chat.identity.resumed_from = wrote.clone();
    }
    let coming = AskerComing::take(held, |record| {
        named(record) || (wrote.is_some() && record.asker.chat.id == wrote)
    });
    let started = start(&chat);
    coming.back(held, started.as_ref().copied());
    started
}

/// **Retry now** (or a task row's **Try to start again**) on the chat with id `id` that a
/// launch could not start (#1513): as [`resuming`], for the reports kept while it was not open,
/// by its id.
///
/// **A task is told to carry on only while its dispatch is at work** (#1546): one whose record
/// another hand ended while it waited is started told nothing, as any chat that did not start
/// is (`dispatchrestart::still_at_work`).
pub(crate) fn retrying(
    held: &Held,
    id: &str,
    start: impl FnOnce() -> Result<u32, String>,
) -> Result<u32, String> {
    let ended = held.chats().waiting_to_start().into_iter().any(|one| {
        one.chat.identity.id.as_deref() == Some(id)
            && one.told.is_some()
            && !dispatchrestart::still_at_work(
                held.root(),
                &dispatchrecord::list(held.root()),
                &one.chat,
            )
    });
    if ended {
        held.chats().tell_nothing(id);
    }
    let told = held
        .chats()
        .waiting_to_start()
        .into_iter()
        .any(|one| one.chat.identity.id.as_deref() == Some(id) && one.told.is_some());
    let started = coming_back_as(held, id, start);
    if let (true, Ok(task)) = (told, &started) {
        questions_were_not_kept(held, *task);
    }
    started
}

/// **Reopen on a finished task's row** (#1546): the task chat with id `id` comes back as a chat
/// of its own, and is handed the reports of the tasks it had asked for that were kept because
/// it had gone, as [`resuming`] hands them. One that lands later reaches it through
/// `Identity::resumed_from` ([`to_a_resumed_asker`]).
pub(crate) fn reopening(
    held: &Held,
    id: &str,
    start: impl FnOnce() -> Result<u32, String>,
) -> Result<u32, String> {
    coming_back_as(held, id, start)
}

/// The chat with id `id` starts: the reports kept for it are taken from the workspace before,
/// and handed to it once it has.
fn coming_back_as(
    held: &Held,
    id: &str,
    start: impl FnOnce() -> Result<u32, String>,
) -> Result<u32, String> {
    let coming = AskerComing::take(held, |record| record.asker.chat.id.as_deref() == Some(id));
    let started = start();
    coming.back(held, started.as_ref().copied());
    started
}

/// The reports kept for a chat that is about to come back, taken from where they wait.
#[must_use = "what was taken goes to the chat once it starts, or back where it was"]
struct AskerComing(Vec<Owing>);

impl AskerComing {
    /// Takes the reports kept for the chat `asked` answers for, under the lock a report is
    /// delivered under.
    fn take(held: &Held, asked: impl Fn(&dispatchrecord::Record) -> bool) -> Self {
        let _deciding = held.chats().deciding();
        Self::take_under(held, asked)
    }

    /// [`Self::take`], under the caller's hold of that lock.
    fn take_under(held: &Held, asked: impl Fn(&dispatchrecord::Record) -> bool) -> Self {
        Self(dispatchrestart::take_back(held.root(), asked))
    }

    /// The chat came back as `session`, or did not start: each report is left for its next
    /// turn, or put back where it was.
    fn back(self, held: &Held, started: Result<u32, &String>) {
        let _deciding = held.chats().deciding();
        match started {
            Ok(session) => {
                dispatchrestart::hand_to(held.root(), &self.0, session);
            }
            Err(_) => dispatchrestart::give_back(held.root(), &self.0),
        }
    }

    /// Each report to the open chat its record names, by id, or that resumed that chat; the
    /// rest put back where they were. Under the caller's hold of the lock.
    fn back_to_open(self, held: &Held) {
        let open: Vec<(u32, Known)> = held
            .chats()
            .open_now()
            .into_iter()
            .filter_map(|one| {
                let chat = held.chats().recorded_chat(one.session)?;
                Some((one.session, Known::of(&chat)))
            })
            .collect();
        let mut not_open = Vec::new();
        for one in self.0 {
            let own = |known: &Known| one.asker.id.is_some() && known.id == one.asker.id;
            let resumed =
                |known: &Known| one.asker.id.is_some() && known.resumed_from == one.asker.id;
            let to = open
                .iter()
                .find(|(_, known)| own(known))
                .or_else(|| open.iter().find(|(_, known)| resumed(known)));
            match to {
                Some((session, _)) => {
                    dispatchrestart::hand_to(held.root(), std::slice::from_ref(&one), *session);
                }
                None => not_open.push(one),
            }
        }
        dispatchrestart::give_back(held.root(), &not_open);
    }
}
