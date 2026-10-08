//! The app's half of tasks across a restart, and of a task whose asking chat has gone (#1513,
//! V100-63, V100-64). The rules are `purlis_core::dispatchrestart`'s; this module feeds them
//! what only the app has (which chats it opened, which wait to start, which it is about to
//! start) and does what they answer.
//!
//! # At a launch
//!
//! [`before_put_back`] reads the dispatch store once and says, of the record of open chats,
//! which tasks still owing a report are told to carry on as they start, and which cannot be
//! resumed and are left out. [`after_put_back`] then settles what the launch could not bring
//! back: a task left out, or one whose start was refused, has ended by itself, and the chat
//! that asked is told, once, and its row is a finished one with Reopen; the task leaves the
//! list of chats that did not start, so it is never started again as a task. A dispatch whose
//! chat neither came back nor waits to has ended. **Nothing is dispatched a second time**: a
//! task comes back on its conversation, told one fixed sentence, never its brief; and a brief
//! its asking chat sends again after the restart, while the task it started before is still
//! at work, is refused ([`refuse_twice`]).
//!
//! # A report whose asking chat has gone
//!
//! A task's report kept for its workspace because the chat that asked had gone, or left for
//! that chat and unread when it closed, is marked on the task's dispatch record
//! ([`kept_for_its_asker`], [`unread_at_close`]). When that chat comes back (a launch, Retry
//! now on a chat that did not start, Resume from its session record), every such report is
//! taken back from the workspace before the chat starts and handed to it once it has
//! ([`AskerComing::back`]).
//!
//! Every hand-over runs under `Chats::deciding()`, the lock a report is delivered under, so a
//! report that lands while an asking chat starts goes either to it or onto its record, and is
//! taken from there.

use std::path::Path;

use purlis_core::dispatchrecord::{self, ChatRef};
use purlis_core::dispatchrestart::{self, AtLaunch, Owing};
use purlis_core::reopen::{Chat, Record};

use crate::planes::Held;

/// **Puts back `record` at a launch** (#1513), as `Chats::put_back_telling` does, with what is said
/// above: the chats that came back.
pub(crate) fn put_back(
    held: &Held,
    record: &Record,
    size: purlis_core::engine::Size,
) -> Vec<crate::chats::Open> {
    let launch = before_put_back(held.root(), record);
    let coming = coming_back(held, &launch.back);
    let opened = held
        .chats()
        .put_back_telling(&launch.back, size, &|chat| launch.told(chat));
    after_put_back(held, &launch, coming);
    opened
}

/// What a launch makes of the tasks in `record`, before anything is started: read once.
fn before_put_back(root: &Path, record: &Record) -> AtLaunch {
    let launch = dispatchrestart::at_launch(record, &dispatchrecord::list(root));
    for chat in &launch.not_resumed {
        tracing::info!(
            "purlis: task chat '{}' had no conversation to bring back; it has ended, and the \
             chat that asked is told",
            chat.label.as_deref().unwrap_or(&chat.name)
        );
    }
    launch
}

/// The reports kept for chats `record` brings back, taken from their workspaces before any of
/// those chats starts: handed to each once it has ([`after_put_back`]).
fn coming_back(held: &Held, record: &Record) -> AskerComing {
    let ids: Vec<String> = record
        .chats
        .iter()
        .filter_map(|chat| chat.identity.id.clone())
        .collect();
    AskerComing::take(held, move |asker| {
        asker.chat.id.as_ref().is_some_and(|id| ids.contains(id))
    })
}

/// **After the launch's put-back** (#1513): what could not be brought back has ended by itself
/// and its asking chat is told; the reports kept for chats that came back are handed to them.
fn after_put_back(held: &Held, launch: &AtLaunch, coming: AskerComing) {
    let root = held.root();
    let now = chrono::Utc::now();
    let deciding = held.chats().deciding();
    let open = held.chats().open_now();
    let asker_open = |chat: &Chat| {
        chat.from
            .as_ref()
            .is_some_and(|from| open.iter().any(|one| one.session == from.chat))
    };
    let mut told = Vec::new();
    for chat in &launch.not_resumed {
        told.extend(end_it(root, chat, asker_open(chat), None, now));
    }
    // A task that was tried and refused has ended too, and leaves the list of chats that did
    // not start: Retry now would start it as a task a second time, owing a report its asking
    // chat has had. Its row under that chat offers Reopen.
    for waiting in held.chats().would_not_start() {
        if !launch.is_a_listed_task(&waiting.id) {
            continue;
        }
        let Some(chat) = launch
            .back
            .chats
            .iter()
            .find(|chat| chat.identity.id.as_deref() == Some(waiting.id.as_str()))
        else {
            continue;
        };
        told.extend(end_it(
            root,
            chat,
            asker_open(chat),
            Some(&waiting.why),
            now,
        ));
        if let Err(why) = held.chats().forget(&waiting.id) {
            tracing::warn!("purlis: a task that did not start was not let go of ({why})");
        }
    }
    // A dispatch whose chat neither came back nor waits to start has ended.
    let waiting: Vec<String> = held
        .chats()
        .would_not_start()
        .into_iter()
        .map(|one| one.id)
        .collect();
    let live = |worker: &ChatRef| {
        worker.id.as_ref().is_some_and(|id| {
            waiting.contains(id)
                || open.iter().any(|one| {
                    held.chats()
                        .chat_at(one.session)
                        .and_then(|at| at.id)
                        .as_ref()
                        == Some(id)
                })
        })
    };
    dispatchrestart::settle_after_launch(root, live, now);
    // What was kept for a chat that is open now is handed to it: taken before the put-back,
    // and what a settle above kept for one.
    let settled = AskerComing::take_under(held, |asker| is_open(held, &asker.chat));
    coming.back_to_open(held);
    settled.back_to_open(held);
    drop(deciding);
    for (asker, ended) in told {
        // What waits in the asking chat's folder is a closed task's report: a wait on it
        // reads it, and the asking chat is told a report landed, as for any closed task.
        if let Some(task) = ended.task {
            let mut ledger = held.tasks().ledger();
            ledger.reported(task, asker, ended.report, ended.file);
            ledger.forget(task, Some((asker, &ended.name)));
        }
        crate::dispatched::told(held, asker);
    }
    held.rows_changed();
}

/// [`dispatchrestart::end_at_launch`], with what to tell the ledger where it told the chat
/// that asked.
fn end_it(
    root: &Path,
    chat: &Chat,
    asker_open: bool,
    why: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<(u32, dispatchrestart::Ended)> {
    match dispatchrestart::end_at_launch(root, chat, asker_open, why, now) {
        Ok(Some(ended)) if ended.file.is_some() => Some((ended.asker, ended)),
        Ok(_) => None,
        Err(why) => {
            tracing::warn!(
                "purlis: a task the launch could not bring back was not settled ({why})"
            );
            None
        }
    }
}

/// Whether the chat a record names by `chat` is one this app has open: by its id.
fn is_open(held: &Held, chat: &ChatRef) -> bool {
    chat.id
        .as_ref()
        .is_some_and(|id| held.chats().id_is_open(id))
}

/// **A brief its asking chat already dispatched before the restart is refused** (#1513): the
/// chat `asker`, whose current run began after it dispatched the task, sends `brief` to
/// `persona` again while that task still works. A chat that was not started again since it
/// dispatched is asking for a second task on purpose, and gets one.
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
/// in this launch, else the one its dispatch record names (#1456, #1513). The app's note is
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
/// if the person reopens it. `chat` is the task's chat, still open.
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

/// **Resume from the session record at `path`** (#1513, V100-64): the reports of the tasks the
/// chat that wrote it asked for, kept because it had closed, are taken from the workspace
/// before `start` starts the chat that resumes it, and left for that chat's next turn once it
/// has. By the record the app named on each dispatch when that chat wrote it. What `start`
/// answers is answered.
pub(crate) fn resuming(
    held: &Held,
    path: &str,
    start: impl FnOnce() -> Result<u32, String>,
) -> Result<u32, String> {
    let coming = AskerComing::take(held, |asker| asker.session_record.as_deref() == Some(path));
    let started = start();
    coming.back(held, started.as_ref().copied());
    started
}

/// **Retry now** on the chat with id `id` that a launch could not start (#1513): as
/// [`resuming`], for the reports kept while it was not open, by its id.
pub(crate) fn retrying(
    held: &Held,
    id: &str,
    start: impl FnOnce() -> Result<u32, String>,
) -> Result<u32, String> {
    let coming = AskerComing::take(held, |asker| asker.chat.id.as_deref() == Some(id));
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
    fn take(held: &Held, asked: impl Fn(&dispatchrecord::Asker) -> bool) -> Self {
        let _deciding = held.chats().deciding();
        Self::take_under(held, asked)
    }

    /// [`Self::take`], under the caller's hold of that lock.
    fn take_under(held: &Held, asked: impl Fn(&dispatchrecord::Asker) -> bool) -> Self {
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

    /// Each report to the open chat its record names, by id; the rest put back where they
    /// were. Under the caller's hold of the lock.
    fn back_to_open(self, held: &Held) {
        let mut not_open = Vec::new();
        for one in self.0 {
            let session = held.chats().open_now().into_iter().find(|open| {
                one.asker.id.is_some()
                    && held.chats().chat_at(open.session).and_then(|at| at.id) == one.asker.id
            });
            match session {
                Some(open) => {
                    dispatchrestart::hand_to(held.root(), std::slice::from_ref(&one), open.session);
                }
                None => not_open.push(one),
            }
        }
        dispatchrestart::give_back(held.root(), &not_open);
    }
}
