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
//!   reopened, or when the chat that asked closes ([`asker_closed`]). **Clearing takes the row
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
    pub persona: Option<String>,
    /// How it ended: `done`, `cancelled`, `blocked`, `failed`, `ended without a report` or
    /// `closed by the person`.
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
        persona: record.persona.clone(),
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
    }
}

/// The finished tasks of every chat this app has open, oldest first: one read of the store.
pub(crate) fn listed(held: &Held) -> Vec<FinishedTask> {
    let open = crate::dispatches::open_chats(held);
    let is = |chat: &dispatchrecord::ChatRef| {
        open.iter()
            .find(|one| dispatchrecord::named(chat, one.id.as_deref(), Some(one.session)))
            .map(|one| one.session)
    };
    dispatchrecord::finished(held.root(), |worker| is(worker).is_some())
        .iter()
        .filter_map(|record| Some(row(record, is(&record.asker.chat)?, Finished::of(record)?)))
        .collect()
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
        open.iter()
            .any(|one| dispatchrecord::named(worker, one.id.as_deref(), Some(one.session)))
    })
    .iter()
    .filter_map(|record| {
        let how = Finished::of(record)?;
        let number = record.worker.chat.chat;
        let name = name_of(record);
        // The ledger's own memory of the tasks that closed in this launch says whether that
        // number is still this task's.
        let this_launch = held
            .tasks()
            .ledger()
            .gone(asker, number)
            .is_some_and(|gone| gone.name == name);
        Some(purlis_core::dispatched::Row {
            chat: if this_launch { number } else { 0 },
            name,
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
pub(crate) fn asker_closed(held: &Held, session: u32) {
    let Some(me) = crate::dispatches::chat_ref(held, session) else {
        return;
    };
    if crate::dispatches::carried_on(held, session, &me) {
        return;
    }
    dispatchrecord::clear_for(held.root(), &me);
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
    if open
        .iter()
        .any(|one| dispatchrecord::named(&record.worker.chat, one.id.as_deref(), Some(one.session)))
    {
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
/// new chat's number. Its row goes: it is a chat with a tab now. The chat that asked is told
/// nothing, here or later.
pub(crate) fn reopen(
    held: &Held,
    id: &str,
    size: purlis_core::engine::Size,
) -> Result<u32, String> {
    let (chat, ready) = reopening(held, id)?;
    let session = held.chats().start_ready(&chat, &ready, size)?;
    if let Err(why) = dispatchrecord::clear(held.root(), id) {
        tracing::warn!("purlis: a reopened task's row was not cleared ({why})");
    }
    Ok(session)
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
                persona: Some("devops".to_owned()),
                outcome: "done".to_owned(),
                folds: true,
                // As written: the window draws it as text.
                report: "<b>Healthy</b>, 3 of 3 pods.".to_owned(),
                changed: Some("values.yaml: replicas 2 to 3".to_owned()),
                ended: Some("2026-10-07T12:04:30+00:00".to_owned()),
                place: "beta".to_owned(),
                branch: None,
                reopens: true,
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
