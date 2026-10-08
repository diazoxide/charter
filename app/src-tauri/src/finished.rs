//! The app's half of a finished task (#1485): the rows that stay under the chat that asked,
//! clearing them, and reopening one as an ordinary chat.
//!
//! **A task ends at its report** ([`crate::dispatched`]): its program is ended, and its chat is
//! closed. What stays is its dispatch record (`purlis_core::dispatchrecord`), and everything
//! here is read from it, so the rows are the same after the app is started again.
//!
//! - **The rows** ([`listed`]) are the ended tasks of every chat this app has open, by the
//!   asking chat's id, which a restart and a relaunch keep. Done and cancelled fold into one
//!   line in the window; every other end stays a row of its own
//!   (`dispatchrecord::Finished::folds`). A row goes when it is cleared, when the task is
//!   reopened and its new chat is heard from, or when the chat that asked closes
//!   ([`asker_closed`]). A row is matched to its chats by id, never by number. **Clearing takes the row
//!   and nothing else**: the record stays, and is collected as any record is.
//! - **Reopen** ([`reopen`]) starts a new chat on the conversation the task ended in, on the
//!   profile, persona and folder the record names. **It is an ordinary chat**: the app records
//!   no asking chat for it, so it owes nobody a report, a `purlis dispatch report` from it is
//!   refused in plain words ([`reopened_from`]), and the chat that asked is told nothing. It
//!   holds the persona grants a chat resumed from a session record holds
//!   (`purlis_core::sessionresume::resumed_holds`): never more than the task had.
//!
//! Every command here is the window's, so only the person runs it: no line on the hook
//! channel reaches a clear or a reopen.

use purlis_core::dispatchrecord::{self, Finished, Record};

use crate::planes::Held;

/// One finished task, as its row under the chat that asked draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct FinishedTask {
    /// Its dispatch record's id: what clearing and reopening name it by.
    pub id: String,
    /// The chat that asked, by this app's number for it now.
    pub asker: u32,
    /// The task's name, as its row had it while it worked.
    pub name: String,
    /// The number the task's chat had, where it ended in this launch and that number is still
    /// its own (the ledger's memory of closed tasks says so): what a pane left on that chat
    /// finds its row by. None for a task that finished before this app was started.
    pub chat: Option<u32>,
    pub persona: Option<String>,
    /// **How it ended, as a value** ([`How`]): what the window draws its state from. The
    /// window never reads the sentence in `outcome` to learn this.
    pub how: How,
    /// How it ended, in words: `done`, `cancelled`, `blocked`, `failed`, `ended without a
    /// report` or `closed by the person`.
    pub outcome: String,
    /// Whether it folds into the one "Finished (n)" line: done and cancelled do, and every
    /// other end stays a row of its own until it is cleared (V100-9).
    pub folds: bool,
    /// Its report, as written: the task's words, or purlis's for one that sent none. Text,
    /// and drawn as text.
    pub report: String,
    /// What the report says changed, in the task's words, where it said.
    pub changed: Option<String>,
    /// When it ended, as the record keeps it (UTC, RFC 3339).
    pub ended: Option<String>,
    /// Where it worked: the workspace's name, or `project root`.
    pub place: String,
    /// The branch purlis cut for it, where its dispatch gave it one.
    pub branch: Option<String>,
    /// Whether it can be reopened: its record names the conversation it ended in.
    pub reopens: bool,
    /// Why the last Reopen of it did not hold, where one did not: said on its row.
    pub not_reopened: Option<String>,
}

/// How a finished task ended (`dispatchrecord::Finished`), as the window is sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum How {
    Done,
    Cancelled,
    Blocked,
    Failed,
    /// Its program ended before it reported, and purlis said so in its place.
    Unreported,
    /// The person stopped or closed it.
    StoppedByPerson,
}

impl From<Finished> for How {
    fn from(how: Finished) -> Self {
        match how {
            Finished::Done => Self::Done,
            Finished::Cancelled => Self::Cancelled,
            Finished::Blocked => Self::Blocked,
            Finished::Failed => Self::Failed,
            Finished::EndedWithoutAReport => Self::Unreported,
            Finished::ClosedByThePerson => Self::StoppedByPerson,
        }
    }
}

/// The task's name: the one its dispatch gave it, else its chat's.
fn name_of(record: &Record) -> String {
    record
        .task
        .clone()
        .unwrap_or_else(|| record.worker.chat.name.clone())
}

fn row(record: &Record, asker: u32, how: Finished) -> FinishedTask {
    FinishedTask {
        id: record.id.clone(),
        asker,
        name: name_of(record),
        chat: None,
        persona: record.persona.clone(),
        how: how.into(),
        outcome: how.word().to_owned(),
        folds: how.folds(),
        report: record
            .report
            .as_ref()
            .map(|report| report.text.clone())
            .unwrap_or_default(),
        changed: record
            .report
            .as_ref()
            .and_then(|report| report.changed.said.clone()),
        ended: record.ended.clone(),
        place: record
            .place
            .workspace
            .clone()
            .unwrap_or_else(|| "project root".to_owned()),
        branch: record
            .place
            .worktree
            .as_ref()
            .and_then(|tree| tree.branch.clone()),
        reopens: record.conversation.is_some(),
        not_reopened: None,
    }
}

/// Whether `recorded`, a chat as a record names it, is the open chat `open`: **by its id, and
/// never by its number**. A finished task's record is read across launches, and a number
/// does not stay a chat's own for that long: a chat started again is given a new one, and the
/// count starts over where the record of open chats could not be read (`reopen::Record::dealt`
/// holds it otherwise).
fn is_open(recorded: &dispatchrecord::ChatRef, open: &crate::dispatches::OpenChat) -> bool {
    recorded.id.is_some() && recorded.id == open.id
}

/// The finished tasks of every chat this app has open, oldest first: one read of the store.
/// A task being reopened is not drawn while its new chat starts: it is that chat, or it is a
/// row again with the reason, a moment from now.
pub(crate) fn listed(held: &Held) -> Vec<FinishedTask> {
    let open = crate::dispatches::open_chats(held);
    let is = |chat: &dispatchrecord::ChatRef| {
        open.iter()
            .find(|one| is_open(chat, one))
            .map(|one| one.session)
    };
    dispatchrecord::finished(held.root(), |worker| is(worker).is_some())
        .iter()
        .filter(|record| !held.tasks().reopening(&record.id))
        .filter_map(|record| {
            let asker = is(&record.asker.chat)?;
            Some(FinishedTask {
                chat: ended_as(held, record, asker),
                not_reopened: held.tasks().not_reopened(&record.id),
                ..row(record, asker, Finished::of(record)?)
            })
        })
        .collect()
}

/// The number `record`'s task chat had when it ended, where it ended in this launch under the
/// chat now numbered `asker`: the ledger's own memory of the tasks that closed in this launch
/// says whether that number is still this task's.
fn ended_as(held: &Held, record: &Record, asker: u32) -> Option<u32> {
    let number = record.worker.chat.chat;
    let name = name_of(record);
    held.tasks()
        .ledger()
        .gone(asker, number)
        .is_some_and(|gone| gone.name == name)
        .then_some(number)
}

/// The finished tasks of chat `asker`, as `purlis dispatch list` lists them after its open
/// ones: by the number each task's chat had, where that chat ended in this launch and a wait
/// still reads its report by it; with none (0) for one that finished before this app was
/// started, whose number may be another chat's by now.
pub(crate) fn listed_for(held: &Held, asker: u32) -> Vec<purlis_core::dispatched::Row> {
    let Some(me) = crate::dispatches::chat_ref(held, asker) else {
        return Vec::new();
    };
    let open = crate::dispatches::open_chats(held);
    let now = chrono::Utc::now();
    dispatchrecord::finished_for(held.root(), &me, |worker| {
        open.iter().any(|one| is_open(worker, one))
    })
    .iter()
    .filter(|record| !held.tasks().reopening(&record.id))
    .filter_map(|record| {
        let how = Finished::of(record)?;
        Some(purlis_core::dispatched::Row {
            chat: ended_as(held, record, asker).unwrap_or(0),
            name: name_of(record),
            persona: record.persona.clone(),
            place: record
                .place
                .workspace
                .clone()
                .map_or(
                    purlis_core::active::Place::PlaneRoot,
                    purlis_core::active::Place::Workspace,
                )
                .word()
                .to_owned(),
            state: match how {
                Finished::Done | Finished::Cancelled | Finished::Blocked | Finished::Failed => {
                    format!("reported: {}", how.word())
                }
                Finished::EndedWithoutAReport | Finished::ClosedByThePerson => {
                    how.word().to_owned()
                }
            },
            age_secs: chrono::DateTime::parse_from_rfc3339(&record.started)
                .ok()
                .and_then(|started| (now - started.with_timezone(&chrono::Utc)).to_std().ok())
                .map(|age| age.as_secs()),
            by_person: record.asker.by_person,
            branch: record
                .place
                .worktree
                .as_ref()
                .and_then(|tree| tree.branch.clone()),
            branch_stands: None,
            finished: true,
        })
    })
    .collect()
}

/// Chat `session` is closing: the rows of the finished tasks it asked for go with it
/// (V100-10), unless a chat started again in its place carries it on. Asked while the app
/// still knows the chat. Their records stay.
///
/// **Nothing is written here.** This is called under the lock every dispatch and report waits
/// on, and marking the rows reads the whole store and rewrites a file a row. The rows are
/// not drawn from this moment anyway, since their asking chat is no longer open; the marks are
/// written on a thread of their own, as a closed chat's worktree is looked at.
pub(crate) fn asker_closed(held: &Held, session: u32) {
    let Some(me) = crate::dispatches::chat_ref(held, session) else {
        return;
    };
    // A chat with no id has no rows: a row is matched by its asking chat's id alone.
    if me.id.is_none() || crate::dispatches::carried_on(held, session, &me) {
        return;
    }
    let root = held.root().to_path_buf();
    let cleared = std::thread::Builder::new()
        .name("purlis-finished-rows".into())
        .spawn(move || {
            dispatchrecord::clear_for(&root, &me);
        });
    if let Err(why) = cleared {
        tracing::warn!("purlis: a closed chat's finished rows were not cleared ({why})");
    }
}

/// The person took chat `session` over after it reported, as a task (#1485): its dispatch
/// record says so, so the next launch puts it back as the chat it is and ends nothing.
pub(crate) fn taken_over(held: &Held, session: u32) {
    let Some(me) = crate::dispatches::chat_ref(held, session) else {
        return;
    };
    let Some(record) = dispatchrecord::latest_for(held.root(), &me) else {
        return;
    };
    if let Err(why) = dispatchrecord::kept_open(held.root(), &record.id) {
        tracing::warn!("purlis: a task the person took over was not marked ({why})");
    }
}

/// **The record to put back at a launch**: `record` without the task chats that had finished
/// when the app quit (`dispatched::left_out_at_launch`, #1485). Nothing is started for one:
/// its dispatch record already holds its report and its conversation, so it is a finished row
/// under the chat that asked. Whether one had finished is the app's own dispatch record of
/// it, by the chat's id: ended with a report, not blocked, and not taken over by the person.
///
/// What was still waiting for a chat left out goes where a closed chat's goes: a report
/// waiting for its next turn to its workspace, and its messages away.
pub(crate) fn put_back_without_the_finished(
    root: &std::path::Path,
    record: &purlis_core::reopen::Record,
) -> purlis_core::reopen::Record {
    let finished = |chat: &purlis_core::reopen::Chat| {
        let Some(id) = chat.identity.id.clone() else {
            return false;
        };
        let me = dispatchrecord::ChatRef {
            chat: chat.number.unwrap_or_default(),
            id: Some(id),
            ..Default::default()
        };
        dispatchrecord::latest_for(root, &me).is_some_and(|record| {
            !record.kept_open && Finished::of(&record).is_some_and(|how| how != Finished::Blocked)
        })
    };
    let (back, out) = purlis_core::dispatched::left_out_at_launch(record, finished);
    for chat in &out {
        tracing::info!(
            "purlis: task chat '{}' had reported when the app quit; it is a finished row, and \
             is not started again",
            chat.label.as_deref().unwrap_or(&chat.name)
        );
        if let Some(number) = chat.number {
            purlis_core::handback::orphan(root, number);
            purlis_core::dispatchtalk::forget(root, number);
        }
    }
    back
}

/// The task chat `chat` was reopened from, by name, where it was reopened from one: its own
/// record says which chat it resumed (`Identity::resumed_from`), and that chat's dispatch
/// record says it was a task. What a report from it is refused by.
pub(crate) fn reopened_from(held: &Held, chat: u32) -> Option<String> {
    let was = held.chats().recorded_chat(chat)?.identity.resumed_from?;
    dispatchrecord::task_worked_by(held.root(), &was)
        .filter(|record| Finished::of(record).is_some())
        .map(|record| name_of(&record))
}

/// Clears the rows of the finished tasks `ids`, and answers how many it cleared. The rows and
/// nothing else: each record stays (`dispatchrecord::clear`).
pub(crate) fn clear(held: &Held, ids: &[String]) -> u32 {
    let cleared = ids
        .iter()
        .filter(|id| dispatchrecord::clear(held.root(), id).unwrap_or(false))
        .count();
    u32::try_from(cleared).unwrap_or(u32::MAX)
}

/// The chat a Reopen of the finished task `id` starts, and the launch the core worked out for
/// it; or the one sentence saying why it cannot be reopened.
///
/// **Everything is the record's**, which the app wrote: the conversation the task ended in,
/// the profile and the persona it ran as, and the folder it worked in. The chat names no
/// asking chat (`from: None`), which is the whole of "it is no longer a task".
pub(crate) fn reopening(
    held: &Held,
    id: &str,
) -> Result<(purlis_core::reopen::Chat, purlis_core::start::Ready), String> {
    let root = held.root();
    let record = dispatchrecord::read(root, id)
        .filter(dispatchrecord::sound)
        .ok_or_else(|| "purlis has no record of that task, so it cannot be reopened.".to_owned())?;
    if Finished::of(&record).is_none() {
        return Err(
            "That dispatch is not a finished task, so there is nothing to reopen.".to_owned(),
        );
    }
    let name = name_of(&record);
    let open = crate::dispatches::open_chats(held);
    if open.iter().any(|one| is_open(&record.worker.chat, one)) {
        return Err(format!(
            "'{name}' is still open: its program has not ended yet. Open its chat instead."
        ));
    }
    let conversation = record.conversation.as_deref().ok_or_else(|| {
        format!(
            "'{name}' cannot be reopened: its harness named no conversation for purlis to \
             resume. Its report is still here to read."
        )
    })?;
    let resume = purlis_core::harness::SessionId::new(conversation).map_err(|_| {
        format!(
            "'{name}' cannot be reopened: its record names a conversation purlis will not pass on."
        )
    })?;
    let profile = record.worker.profile.clone().ok_or_else(|| {
        format!("'{name}' cannot be reopened: its record does not say which profile it ran on.")
    })?;
    let folder = record.place.folder.as_deref().unwrap_or(".");
    // As the record's writer says it: relative to the project, or the whole path of a folder
    // outside it. A relative one that climbs out is not one the app wrote.
    let climbs = std::path::Path::new(folder)
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir));
    let cwd = if std::path::Path::new(folder).is_absolute() {
        std::path::PathBuf::from(folder)
    } else {
        root.join(folder)
    };
    if climbs || !cwd.is_dir() {
        return Err(format!(
            "'{name}' cannot be reopened: the folder it worked in is gone ({folder}). Its \
             report is still here to read."
        ));
    }
    // What a chat resumed from a session record holds, and for its reason (#1362): the
    // default persona's grants where this persona's reach past them, until the person allows
    // its own on its tab.
    let held_grants = purlis_core::sessionresume::resumed_holds(root, record.persona.as_deref());
    let ready = purlis_core::start::ready(
        &purlis_core::start::Start {
            profile: Some(profile.clone()),
            persona: record.persona.clone(),
            name: name.clone(),
            cwd: Some(cwd.clone()),
            resume: Some(resume),
            held: held_grants.clone(),
            ..Default::default()
        },
        root,
    )?;
    // **The harness the record's conversation is in must be the one this profile runs now.**
    // A profile pointed at another harness since would be handed an id it cannot resume.
    let runs = ready.harness.map(purlis_core::harness::Harness::name);
    if let Some(ran) = record.worker.harness.as_deref()
        && runs != Some(ran)
    {
        return Err(format!(
            "'{name}' cannot be reopened: it ran on {ran}, and the profile '{profile}' now runs \
             {}. Its conversation is {ran}'s. Its report is still here to read.",
            runs.unwrap_or("no harness purlis knows")
        ));
    }
    let chat = purlis_core::reopen::Chat {
        program: ready.program.clone(),
        args: Vec::new(),
        cwd: Some(cwd),
        name: name.clone(),
        resume: ready.session.clone(),
        profile: Some(profile),
        persona: record.persona.clone(),
        held: held_grants,
        // The task's name is what its row was called, and what its tab is.
        label: Some(name),
        // An ordinary chat: nobody asked for it, and it owes nobody a report. A chat of its
        // own, which says the chat it carries on from.
        from: None,
        identity: purlis_core::reopen::Identity {
            resumed_from: record.worker.chat.id.clone(),
            ..Default::default()
        },
        // A chat the person opened has a tab from its start.
        ..Default::default()
    };
    Ok((chat, ready))
}

/// **Reopens the finished task `id`** as an ordinary chat on its conversation, and answers the
/// new chat's number. The chat that asked is told nothing, here or later.
///
/// **One Reopen of a row at a time, and one for good**: a second press while the first is
/// starting its chat, and a press on a row already cleared, are refused, under one lock
/// (`Tasks::reopen_begins`). So one row is one chat on its conversation.
///
/// **The row is not cleared here.** It is not drawn while the new chat starts, and is cleared
/// when that chat is first heard from, or has lived long enough to have resumed. A chat that
/// ends at once is a harness that could not bring the conversation back: the row is drawn
/// again and says so (`dispatched::NOT_RESUMED`).
pub(crate) fn reopen(
    held: &Held,
    id: &str,
    size: purlis_core::engine::Size,
) -> Result<u32, String> {
    if !held.tasks().reopen_begins(id) {
        return Err("That task is being reopened already: its chat is starting.".to_owned());
    }
    let started = match dispatchrecord::read(held.root(), id) {
        Some(record) if record.cleared => Err(
            "That task was reopened or cleared already, so there is no row to reopen.".to_owned(),
        ),
        _ => reopening(held, id)
            .and_then(|(chat, ready)| held.chats().start_ready(&chat, &ready, size)),
    };
    match started {
        Ok(session) => {
            held.tasks().reopen_started(id, session);
            Ok(session)
        }
        Err(why) => {
            held.tasks().reopen_failed(id);
            Err(why)
        }
    }
}

/// The finished tasks of the project's open chats, oldest first, for the rows under each
/// (#1485). On a blocking thread, as it reads every record.
#[tauri::command]
#[specta::specta]
pub(crate) async fn finished_tasks(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
) -> Result<Vec<FinishedTask>, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading the finished tasks", move || Ok(listed(&held))).await
}

/// **Clear finished** (#1485): takes the rows of the finished tasks `ids` off their chat's
/// list, and answers how many. Nothing else changes: each task's dispatch record stays, with
/// its report.
#[tauri::command]
#[specta::specta]
pub(crate) async fn clear_finished_tasks(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    ids: Vec<String>,
) -> Result<u32, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("clearing the finished tasks", move || {
        Ok(clear(&held, &ids))
    })
    .await
}

#[cfg(test)]
mod tests {
    use purlis_core::dispatchrecord::{
        Asker, Changed, ChatRef, Mode, Outcome, Place, Report, Worker,
    };

    use super::*;

    fn a_task(outcome: Outcome, text: &str) -> Record {
        Record {
            v: dispatchrecord::VERSION,
            id: "01K6DISPATCH00000000000000".to_owned(),
            mode: Mode::Task,
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
            ended: Some("2026-10-07T12:04:30+00:00".to_owned()),
            report: Some(Report {
                outcome,
                text: text.to_owned(),
                changed: Changed {
                    said: Some("values.yaml: replicas 2 to 3".to_owned()),
                    ..Changed::default()
                },
            }),
            needed_you: 0,
            messages: 0,
            usage: None,
            conversation: Some("9f2c-the-conversation".to_owned()),
            cleared: false,
            ended_by: None,
            kept_open: false,
        }
    }

    #[test]
    fn a_finished_task_s_row_says_how_it_ended_and_carries_its_report_as_written() {
        let record = a_task(Outcome::Done, "<b>Healthy</b>, 3 of 3 pods.");
        let drawn = row(&record, 12, Finished::of(&record).unwrap());

        assert_eq!(
            drawn,
            FinishedTask {
                id: "01K6DISPATCH00000000000000".to_owned(),
                // The asking chat by the number it has now, whatever it had then.
                asker: 12,
                name: "check prod".to_owned(),
                // Read from the store alone, as after a restart: no pane was left on it.
                chat: None,
                persona: Some("devops".to_owned()),
                how: How::Done,
                outcome: "done".to_owned(),
                folds: true,
                // As written: the window draws it as text.
                report: "<b>Healthy</b>, 3 of 3 pods.".to_owned(),
                changed: Some("values.yaml: replicas 2 to 3".to_owned()),
                ended: Some("2026-10-07T12:04:30+00:00".to_owned()),
                place: "beta".to_owned(),
                branch: None,
                reopens: true,
                not_reopened: None,
            }
        );
    }

    #[test]
    fn a_failure_is_a_row_of_its_own_and_a_task_with_no_conversation_is_not_reopened() {
        let failed = a_task(Outcome::Failed, "The cluster refused the login.");
        let drawn = row(&failed, 3, Finished::of(&failed).unwrap());
        assert_eq!((drawn.outcome.as_str(), drawn.folds), ("failed", false));

        let unreported = Record {
            conversation: None,
            ended_by: Some(dispatchrecord::EndedBy::Unreported),
            ..a_task(Outcome::Failed, dispatchrecord::ENDED_WITHOUT_A_REPORT)
        };
        let drawn = row(&unreported, 3, Finished::of(&unreported).unwrap());
        assert_eq!(
            (drawn.outcome.as_str(), drawn.folds, drawn.reopens),
            ("ended without a report", false, false)
        );

        let closed = a_task(Outcome::Stopped, purlis_core::handback::STOPPED);
        let drawn = row(&closed, 3, Finished::of(&closed).unwrap());
        assert_eq!(
            (drawn.outcome.as_str(), drawn.folds),
            ("closed by the person", false)
        );
    }
}
