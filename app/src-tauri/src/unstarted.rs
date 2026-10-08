//! The app's half of a task that did not start (#1497, ruling V100-51, D-1497-14).
//!
//! The rules are `purlis_core::didnotstart`'s: the sentence, the record and the report. This
//! module feeds them the app's own record of the chats, on three roads:
//!
//! - **a chat's own dispatch whose start is refused** ([`recorded`], from
//!   `crate::handoff::dispatch_noting`): the profile's program, the sandbox, the
//!   pseudo-terminal, the folder, the worktree. The command that asked is still on the line
//!   and is answered there, as it always was, so the asking chat is told nothing a second
//!   time. What is new is the row: failed, under the chat that asked, with the reason.
//! - **a dispatch the person allowed that then did not start** ([`allowed`], from
//!   `crate::handoff::answered`): the asking chat's command returned long ago, so it is told
//!   as it is told a task's report, and gets the same row.
//! - **a task a launch could not start again** ([`waiting`]). **A launch ends nothing**: the
//!   reason is a condition of the machine at that moment (a profile waiting on the person's
//!   approval, a harness mid-update, a volume not mounted), and the task is still a task. It
//!   stays in the reopen record and is tried again at the next launch. What changes is where
//!   it is drawn: under the chat that asked ([`rows`]), with the reason and the ways out the
//!   window's line had (try to start again, review and approve, end task), and not across
//!   the window. The chat that asked is sent nothing; a wait on it and its list answer a
//!   standing ([`standing`], [`listed_for`]). **Only the person ends it** ([`end`]): then its
//!   record ends failed, it leaves the reopen record, and the chat that asked is told once.
//!
//! **The row of an ended one is the dispatch record's** (`crate::finished`): a record that
//! ends failed and marked `did_not_start` is a finished row that never folds, is cleared as
//! the others are, and is there after the app is started again.
//!
//! **The number.** A task is known to the chat that asked by a chat's number
//! (`purlis dispatch wait <n>`, `list`). One that did not start was dealt a number that no
//! chat ever ran under, or had one before the launch, or is dealt one here: its report is
//! remembered under it as a closed task's is, so a wait on it is answered at once.

use purlis_core::active::Place;
use purlis_core::didnotstart::{self, NotPutBack};
use purlis_core::dispatchrecord::{self, Asker, ChatRef, Opening, Record, Worker};

use crate::chats::{NeedsApproval, WaitingChat};
use crate::planes::Held;

/// A task that was let through and did not start, as the app knew it at that moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unstarted {
    /// The chat that asked, by this app's number for it.
    pub asker: u32,
    /// The number the task was dealt.
    pub number: u32,
    /// The task's name.
    pub name: String,
    pub persona: Option<String>,
    /// The profile it was to start on, where one had been chosen.
    pub profile: Option<String>,
    /// Whether the person asked for it, from the asking chat's tab.
    pub by_person: bool,
    /// The brief, as it was written.
    pub brief: String,
    /// The workspace it was to work in; none for the project's root.
    pub workspace: Option<String>,
    /// The folder it was to start in, where that was settled.
    pub folder: Option<std::path::PathBuf>,
    /// The dispatch record's id, where one was minted for its worktree.
    pub id: Option<String>,
    /// The worktree cut for it, where one was cut and could not be taken back whole: what
    /// is still on the disk for the person to find.
    pub worktree: Option<dispatchrecord::Worktree>,
    /// The id of the chat it was, for a task that had run before a launch could not start it
    /// again; none for a task no chat ever was.
    pub was: Option<String>,
}

/// Where chat `chat` works, by this app's record of it.
fn works_in(held: &Held, chat: u32) -> Option<String> {
    held.chats()
        .recorded_chat(chat)
        .and_then(|asking| asking.cwd)
        .and_then(|cwd| crate::handoff::workspace_of(held.root(), &cwd))
}

fn place(workspace: Option<&str>) -> Place {
    workspace.map_or(Place::PlaneRoot, |name| Place::Workspace(name.to_owned()))
}

/// **Writes the record of `task`, which did not start for `why`**, and tells the window its
/// rows changed. Answers the record where it was kept. Nothing is said to any chat here.
pub(crate) fn recorded(held: &Held, task: &Unstarted, why: &str) -> Option<Record> {
    let root = held.root();
    // Said here, on every road: the row carries the reason, and the log is where it is when
    // the row could not be written.
    tracing::info!("purlis: task '{}' did not start ({why})", task.name);
    let asker = crate::dispatches::chat_ref(held, task.asker)?;
    let opening = Opening {
        mode: dispatchrecord::Mode::Task,
        asker: Asker {
            chat: asker,
            workspace: works_in(held, task.asker),
            by_person: task.by_person,
            session_record: None,
        },
        persona: task.persona.clone(),
        // The chat it would have been: a number and no id for one no chat ever was, so the
        // number is never taken for another chat's (`dispatchrecord::never_a_chat`).
        worker: Worker {
            chat: ChatRef {
                chat: task.number,
                id: task.was.clone(),
                name: task.name.clone(),
                persona: task.persona.clone(),
            },
            harness: None,
            profile: task.profile.clone(),
            session_record: None,
        },
        task: Some(task.name.clone()),
        place: dispatchrecord::Place {
            workspace: task.workspace.clone(),
            folder: task
                .folder
                .as_deref()
                .map(|folder| crate::dispatches::folder(root, folder)),
            // Named only where it is still there: one taken back whole is nothing to find.
            worktree: task.worktree.clone(),
        },
        brief: task.brief.clone(),
        report_owed: true,
    };
    match didnotstart::record(root, task.id.clone(), opening, why, chrono::Utc::now()) {
        Ok(record) => {
            held.rows_changed();
            Some(record)
        }
        Err(kept) => {
            tracing::warn!(
                "purlis: '{}' did not start, and its record was not written ({kept})",
                task.name
            );
            None
        }
    }
}

/// **Tells the open chat `asker` that its task did not start**, as it is told a task's
/// report: failed, in purlis's words with `why` quoted, left for its next turn, answered to a
/// command waiting on `number`, and typed about where the chat is waiting and may be typed a
/// line. A chat that has closed is told nothing: nothing will prompt it again.
///
/// Never called under `Chats::deciding()`: a line may be typed.
fn told(held: &Held, asker: u32, number: u32, task: &Told<'_>, why: &str) {
    let Some(to) = held.chats().shown_name(asker) else {
        return;
    };
    let report = didnotstart::report(
        &didnotstart::Task {
            name: task.name,
            asker: &to,
            asked_from: place(works_in(held, asker).as_deref()),
            place: place(task.workspace),
            by_person: task.by_person,
        },
        why,
    );
    let left = didnotstart::tell(
        held.root(),
        &mut held.tasks().ledger(),
        asker,
        number,
        &report,
    );
    if let Err(kept) = left {
        tracing::warn!(
            "purlis: '{}' did not start, and the chat that asked could not be told ({kept})",
            task.name
        );
        return;
    }
    // Its row says who reported back, as for any report, and whoever waits looks again.
    held.reported_back(asker, task.name);
    crate::dispatched::moved(held, asker);
}

/// What [`told`] says of the task.
struct Told<'a> {
    name: &'a str,
    workspace: Option<&'a str>,
    by_person: bool,
}

/// **The person allowed a dispatch, and its task still did not start** for `why`
/// (`crate::handoff::answered`). `row` says the start itself was refused and its row is
/// written; otherwise it was refused before a start was tried (a limit filled meanwhile, the
/// grant no longer covers it) and its row is written here. Either way the asking chat is told
/// once, as a failed report.
pub(crate) fn allowed(held: &Held, task: &Unstarted, row: bool, why: &str) {
    if !row && recorded(held, task, why).is_none() {
        // No row could be kept: the report still says it, and is what the chat acts on.
        tracing::warn!("purlis: '{}' did not start, and has no row", task.name);
    }
    told(
        held,
        task.asker,
        task.number,
        &Told {
            name: &task.name,
            workspace: task.workspace.as_deref(),
            by_person: task.by_person,
        },
        why,
    );
}

// ----------------------------------------------------------------------------------------
// a task a launch could not start again
// ----------------------------------------------------------------------------------------

/// A task a launch could not start again, whose asking chat is open: drawn under that chat,
/// and still one of the chats waiting to start.
#[derive(Debug, Clone)]
pub(crate) struct Waits {
    /// The chat's own id (ADR 0066): what trying again, approving and ending name it by.
    pub id: String,
    /// The chat that asked, by this app's number for it now.
    pub asker: u32,
    /// The number the task had, which the chat that asked knows it by; 0 where the record
    /// kept none.
    pub number: u32,
    /// The task's name.
    pub name: String,
    /// Why it did not start, as the last try said.
    pub why: String,
    /// The approval its profile needs before it can start, where it needs one (#1246).
    pub approval: Option<NeedsApproval>,
    held: WaitingChat,
    /// Its dispatch's record, where it has one.
    record: Option<Record>,
}

/// **The tasks a launch could not start again, each under an asking chat that is open.**
/// Read from the chats waiting to start every time it is asked: nothing here is a list of
/// its own, so a try that started one, or a Forget, is seen at once.
///
/// A chat the person opened, a handoff's chat, a task that had already reported and a task
/// whose asking chat did not come back either are not here: they stay the window's own line.
pub(crate) fn waiting(held: &Held) -> Vec<Waits> {
    let root = held.root();
    let open = crate::dispatches::open_chats(held);
    let now_open = |chat: &ChatRef| {
        open.iter()
            .find(|one| dispatchrecord::named(chat, one.id.as_deref(), Some(one.session)))
            .map(|one| one.session)
    };
    held.chats()
        .waiting_to_start()
        .into_iter()
        .filter_map(|waiting| {
            let chat = &waiting.chat;
            let id = chat.identity.id.clone()?;
            let name = chat.label.clone().unwrap_or_else(|| chat.name.clone());
            let worker = ChatRef {
                chat: chat.number.unwrap_or_default(),
                id: Some(id.clone()),
                name: name.clone(),
                persona: chat.persona.clone(),
            };
            let record = dispatchrecord::latest_for(root, &worker);
            // The chat that asked, where it is open now: by the dispatch's record of it,
            // which names it by an id a launch keeps; else by the number the task's own
            // record has.
            let asker = match (&record, &chat.from) {
                (Some(record), _) => now_open(&record.asker.chat),
                (None, Some(from)) => open
                    .iter()
                    .find(|one| one.session == from.chat)
                    .map(|one| one.session),
                (None, None) => None,
            }?;
            let number = chat.number.unwrap_or_default();
            let is_a_task = NotPutBack::of(chat.from.as_ref(), true) == NotPutBack::Failed;
            is_a_task.then(|| Waits {
                id,
                asker,
                number,
                name,
                why: waiting.why.clone(),
                approval: waiting.approval.clone(),
                held: waiting,
                record,
            })
        })
        .collect()
}

/// **The chats that did not start which the window says so of in a line across itself**: every
/// one a launch could not start but the tasks of [`waiting`], which are drawn under the chat
/// that asked. So that line is kept for a chat nobody asked for.
pub(crate) fn across_the_window(held: &Held) -> Vec<crate::chats::NotStarted> {
    let under: Vec<String> = waiting(held).into_iter().map(|task| task.id).collect();
    held.chats()
        .would_not_start()
        .into_iter()
        .filter(|one| !under.contains(&one.id))
        .collect()
}

/// The tasks of [`waiting`], as rows under the chat that asked: failed in shape, with the
/// reason, and the ways out a chat that did not start has.
pub(crate) fn rows(held: &Held) -> Vec<crate::finished::FinishedTask> {
    waiting(held)
        .into_iter()
        .map(|task| crate::finished::FinishedTask {
            // The chat's id, not a record's: this row is a chat waiting to start.
            id: task.id,
            asker: task.asker,
            name: task.name,
            // No pane was left on it: it never ran in this launch, so no ended view finds
            // this row (it is found by a chat's number, and this has none).
            chat: None,
            persona: task.held.chat.persona.clone(),
            how: crate::finished::How::Failed,
            outcome: "failed".to_owned(),
            folds: false,
            report: didnotstart::said(&task.why),
            changed: None,
            ended: None,
            place: task
                .held
                .chat
                .cwd
                .as_deref()
                .and_then(|cwd| crate::handoff::workspace_of(held.root(), cwd))
                .unwrap_or_else(|| "project root".to_owned()),
            branch: task
                .record
                .as_ref()
                .and_then(|record| record.place.worktree.as_ref())
                .and_then(|tree| tree.branch.clone()),
            reopens: false,
            not_reopened: None,
            did_not_start: true,
            attempts: 0,
            waits: Some(crate::finished::WaitsToStart {
                approval: task.approval,
            }),
        })
        .collect()
}

/// Chat `asker`'s tasks that a launch could not start again, as its own list has them: by
/// the number each had, with the standing a task waiting on the person has.
pub(crate) fn listed_for(held: &Held, asker: u32) -> Vec<purlis_core::dispatched::Row> {
    waiting(held)
        .into_iter()
        .filter(|task| task.asker == asker)
        .map(|task| purlis_core::dispatched::Row {
            chat: task.number,
            name: task.name,
            persona: task.held.chat.persona.clone(),
            place: task
                .held
                .chat
                .cwd
                .as_deref()
                .and_then(|cwd| crate::handoff::workspace_of(held.root(), cwd))
                .map_or(Place::PlaneRoot, Place::Workspace)
                .word()
                .to_owned(),
            state: didnotstart::waiting_on_the_person(&task.why),
            age_secs: None,
            by_person: task
                .held
                .chat
                .from
                .as_ref()
                .is_some_and(|from| from.by_person),
            branch: None,
            branch_stands: None,
            finished: false,
        })
        .collect()
}

/// Where task number `of` stands for the chat `asker` that asked for it, where it is one a
/// launch could not start again: its name and its standing. What a wait on it is answered,
/// at once: it is not running, it has not reported, and it is not somebody else's.
pub(crate) fn standing(held: &Held, asker: u32, of: u32) -> Option<(String, String)> {
    waiting(held)
        .into_iter()
        .find(|task| task.asker == asker && task.number == of && of != 0)
        .map(|task| (task.name, didnotstart::waiting_on_the_person(&task.why)))
}

/// **End task** (#1497, D-1497-14): the person ends the task `id`, which a launch could not
/// start again. Its dispatch record ends failed with why and keeps its conversation, so
/// Reopen on its row carries it on; it leaves the reopen record; and the chat that asked is
/// told once, as a failed report.
///
/// **Out of the reopen record first, then the record ends**, so the app dying between the two
/// leaves a record the next launch settles, never a chat put back with its dispatch already
/// ended. **And nothing is lost where the record does not end**: the chat is put back among
/// the ones waiting to start, and the person is told why here.
pub(crate) fn end(held: &Held, id: &str) -> Result<(), String> {
    let task = waiting(held)
        .into_iter()
        .find(|task| task.id == id)
        .ok_or_else(|| "That task is no longer waiting to start.".to_owned())?;
    let taken = held
        .chats()
        .take_waiting(id)
        .ok_or_else(|| "That task is no longer waiting to start.".to_owned())?;
    let chat = &taken.chat;
    let root = held.root();
    let by_person = chat.from.as_ref().is_some_and(|from| from.by_person);
    let workspace = chat
        .cwd
        .as_deref()
        .and_then(|cwd| crate::handoff::workspace_of(root, cwd));
    let conversation = chat
        .resume
        .as_ref()
        .map(|resume| resume.as_str().to_owned());
    // The number it had; one dealt now where the record kept none.
    let number = chat
        .number
        .filter(|number| *number != 0)
        .unwrap_or_else(|| held.chats().sessions().deal());
    let ended = match &task.record {
        Some(record) => didnotstart::not_put_back(
            root,
            record,
            &task.why,
            conversation.as_deref(),
            chrono::Utc::now(),
        )
        .map_err(|kept| kept.to_string())
        .and_then(|ended| {
            ended
                .then_some(())
                .ok_or_else(|| "its dispatch's record had already ended".to_owned())
        }),
        // A task from before dispatches kept records: one is written for it now, under the
        // chat it was, with the conversation it was in.
        None => recorded(
            held,
            &Unstarted {
                asker: task.asker,
                number,
                name: task.name.clone(),
                persona: chat.persona.clone(),
                profile: chat.profile.clone(),
                by_person,
                brief: String::new(),
                workspace: workspace.clone(),
                folder: chat.cwd.clone(),
                id: None,
                worktree: None,
                was: Some(id.to_owned()),
            },
            &task.why,
        )
        .ok_or_else(|| "its record could not be written".to_owned())
        .map(|record| {
            // It has ended either way: a conversation that could not be written costs the
            // row its Reopen, and never the ending.
            if let Some(conversation) = conversation.as_deref() {
                let _ = dispatchrecord::ended_in(root, &record.id, conversation);
            }
        }),
    };
    if let Err(why) = ended {
        // Still a task waiting to start, as it was: nothing ended, so nothing is forgotten.
        held.chats().keep_waiting(taken);
        held.rows_changed();
        return Err(format!(
            "'{}' could not be ended ({why}). It is still recorded, and will be tried again at \
             the next launch.",
            task.name
        ));
    }
    tracing::info!(
        "purlis: the person ended task '{}', which did not start again ({})",
        task.name,
        task.why
    );
    held.rows_changed();
    told(
        held,
        task.asker,
        number,
        &Told {
            name: &task.name,
            workspace: workspace.as_deref(),
            by_person,
        },
        &task.why,
    );
    Ok(())
}
