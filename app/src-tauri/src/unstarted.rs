//! The app's half of a task that did not start (#1497, ruling V100-51).
//!
//! The rules are `purlis_core::didnotstart`'s: the sentence, the record and the report. This
//! module feeds them the app's own record of the chats, and is called from the three places a
//! task that was let through can still start no chat:
//!
//! - **a chat's own dispatch whose start is refused** ([`recorded`], from
//!   `crate::handoff::dispatch_it`): the profile's program, the sandbox, the pseudo-terminal,
//!   the folder, the worktree. The command that asked is still on the line and is answered
//!   there, as it always was, so the asking chat is told nothing a second time. What is new
//!   is the row: failed, under the chat that asked, with the reason.
//! - **a dispatch the person allowed that then did not start** ([`allowed`], from
//!   `crate::handoff::answered`): the asking chat's command returned long ago, so it is told
//!   as it is told a task's report, and gets the same row.
//! - **a task a launch could not put back** ([`not_put_back`], from the project's reopen):
//!   the same row and the same report, and the chat leaves the list the window draws as
//!   "did not start", which is kept for chats nobody asked for.
//!
//! **The row is the dispatch record's** (`crate::finished`), so nothing here is a list of its
//! own: a record that ends failed and marked `did_not_start` is a finished row that never
//! folds, is cleared as the others are, and is there after the app is started again.
//!
//! **The number.** A task is known to the chat that asked by a chat's number
//! (`purlis dispatch wait <n>`, `list`). One that did not start was dealt a number that no
//! chat ever ran under, or had one before the launch, or is dealt one here: its report is
//! remembered under it as a closed task's is, so a wait on it is answered at once.

use purlis_core::active::Place;
use purlis_core::didnotstart::{self, NotPutBack};
use purlis_core::dispatchrecord::{self, Asker, ChatRef, Opening, Worker};

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
/// rows changed. Answers whether the record was kept. Nothing is said to any chat here.
pub(crate) fn recorded(held: &Held, task: &Unstarted, why: &str) -> bool {
    let root = held.root();
    let Some(asker) = crate::dispatches::chat_ref(held, task.asker) else {
        return false;
    };
    let opening = Opening {
        mode: dispatchrecord::Mode::Task,
        asker: Asker {
            chat: asker,
            workspace: works_in(held, task.asker),
            by_person: task.by_person,
            session_record: None,
        },
        persona: task.persona.clone(),
        // The chat it would have been: a number, and no id, because no chat ever was.
        worker: Worker {
            chat: ChatRef {
                chat: task.number,
                id: None,
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
            // A worktree cut for it was taken back when it did not start.
            worktree: None,
        },
        brief: task.brief.clone(),
        report_owed: true,
    };
    match didnotstart::record(root, task.id.clone(), opening, why, chrono::Utc::now()) {
        Ok(_) => {
            rows_changed(held, task.number);
            true
        }
        Err(kept) => {
            tracing::warn!(
                "purlis: '{}' did not start ({why}), and its record was not written ({kept})",
                task.name
            );
            false
        }
    }
}

/// Tells the window the finished rows changed, by the one event that already makes it read
/// them again (`chat-stop`, as a task ended at its report tells it): `number` is no open
/// chat's, so there is no tab for it to take away.
fn rows_changed(held: &Held, number: u32) {
    held.tell_stop(number, crate::stopping::StopPhase::Stopped);
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
    if !row && !recorded(held, task, why) {
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

/// **The chats a launch could not put back that were tasks** (V100-63): each that still owed
/// its report, under an asking chat that did come back, ends failed with why
/// (`didnotstart::not_put_back`), is a row under that chat, and that chat is told. It leaves
/// the list of chats waiting to start, so the window draws no banner for it and no later
/// launch tries it again: Reopen on its row is how its conversation is carried on.
///
/// One that had reported before the quit leaves the list too, and nothing more is said: it
/// finished, and its row says how. Every other chat stays as it was, the person's to retry or
/// forget.
pub(crate) fn not_put_back(held: &Held) {
    let root = held.root();
    let open = crate::dispatches::open_chats(held);
    let now_open = |chat: &ChatRef| {
        open.iter()
            .find(|one| dispatchrecord::named(chat, one.id.as_deref(), Some(one.session)))
            .map(|one| one.session)
    };
    for (chat, why) in held.chats().waiting_to_start() {
        let Some(id) = chat.identity.id.clone() else {
            continue;
        };
        let name = chat.label.clone().unwrap_or_else(|| chat.name.clone());
        let worker = ChatRef {
            chat: chat.number.unwrap_or_default(),
            id: Some(id.clone()),
            name: name.clone(),
            persona: chat.persona.clone(),
        };
        let record = dispatchrecord::latest_for(root, &worker);
        // The chat that asked, where it is open now: by the dispatch's record of it, which
        // names it by an id a launch keeps; else by the number the task's own record has.
        let asker = match (&record, &chat.from) {
            (Some(record), _) => now_open(&record.asker.chat),
            (None, Some(from)) => open
                .iter()
                .find(|one| one.session == from.chat)
                .map(|one| one.session),
            (None, None) => None,
        };
        let what = NotPutBack::of(chat.from.as_ref(), asker.is_some());
        let (Some(asker), NotPutBack::Failed | NotPutBack::FinishedAlready) = (asker, what) else {
            continue;
        };
        let by_person = chat.from.as_ref().is_some_and(|from| from.by_person);
        let workspace = chat
            .cwd
            .as_deref()
            .and_then(|cwd| crate::handoff::workspace_of(root, cwd));
        // The number it had, which the chat that asked knows it by; one dealt now where the
        // record kept none.
        let number = chat
            .number
            .filter(|number| *number != 0)
            .unwrap_or_else(|| held.chats().sessions().deal());
        let failed = what == NotPutBack::Failed
            && match &record {
                Some(record) => didnotstart::not_put_back(
                    root,
                    record,
                    &why,
                    chat.resume.as_ref().map(|resume| resume.as_str()),
                    chrono::Utc::now(),
                )
                .unwrap_or_else(|kept| {
                    tracing::warn!(
                        "purlis: {name} did not start, and its record did not end ({kept})"
                    );
                    false
                }),
                // A task from before dispatches kept records: one is written for it now.
                None => recorded(
                    held,
                    &Unstarted {
                        asker,
                        number,
                        name: name.clone(),
                        persona: chat.persona.clone(),
                        profile: chat.profile.clone(),
                        by_person,
                        brief: String::new(),
                        workspace: workspace.clone(),
                        folder: chat.cwd.clone(),
                        id: None,
                    },
                    &why,
                ),
            };
        // Out of the chats waiting to start whether or not it was told: it is a finished
        // task either way, and a banner for it is what this replaces.
        if let Err(kept) = held.chats().forget(&id) {
            tracing::warn!("purlis: {name} is still listed as waiting to start ({kept})");
        }
        if failed {
            tracing::info!("purlis: task {name} was not put back ({why}); its asking chat is told");
            told(
                held,
                asker,
                number,
                &Told {
                    name: &name,
                    workspace: workspace.as_deref(),
                    by_person,
                },
                &why,
            );
        }
    }
}
