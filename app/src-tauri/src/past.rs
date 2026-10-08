//! The app's half of **Past tasks** (#1510, V100-52): what was dispatched in a workspace, read
//! after the task has ended and whether or not the session that asked is still open.
//!
//! A task ends at its report ([`crate::dispatched`]) and stays as a finished row under the
//! chat that asked ([`crate::finished`]) until that row is cleared or the chat closes
//! (V100-10). **This is where it is read after that**: every task of one workspace whose
//! dispatch has ended, newest first, from the dispatch records
//! (`purlis_core::dispatchrecord::past`). Cleared or not.
//!
//! - **Only this machine's records for this project are read.** The store is the app's own
//!   state, never committed, so the list is what this machine saw.
//! - **A record that does not read is skipped and counted**, and one purlis will not draw is
//!   counted apart: neither is tidied and shown.
//! - **The list is bounded** (`dispatchrecord::MOST_PAST`), and says how many older tasks it
//!   left out. A row carries what a row shows; the brief and the report are read for the one
//!   row that is opened ([`read`]).
//! - **A later read does not read everything** ([`listed`] with a [`PastSince`]): only the records
//!   written since, and the ones that were waiting for their chat to close.
//! - **Reopen is the finished row's** ([`crate::finished::reopen`]), offered where that path
//!   would take it. Nothing else acts on a past task.

use purlis_core::dispatchrecord::{self, Finished, Record};

use crate::finished::How;
use crate::planes::Held;

/// One past task, as its row in the Past tasks view draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PastTask {
    /// Its dispatch record's id: what reading it and reopening it name it by.
    pub id: String,
    /// The task's name.
    pub name: String,
    /// The persona that ran it; `null` for a chat started as none.
    pub persona: Option<String>,
    /// The chat that asked, by the name the person saw.
    pub asker: String,
    /// The persona the asking chat ran as.
    pub asker_persona: Option<String>,
    /// Whether the person dispatched it themselves, from that chat's tab.
    pub by_person: bool,
    /// How it ended, as a value: what the window draws its state from, as a finished row does.
    pub how: How,
    /// How it ended, in the core's words.
    pub outcome: String,
    /// When it started and when it ended, as the record keeps them (UTC, RFC 3339).
    pub started: String,
    pub ended: Option<String>,
    /// How long it ran, spelled (`4m 30s`). Empty where its times do not read.
    pub duration: String,
    /// Where it worked: the workspace's name, or `project root`.
    pub place: String,
    /// Where it was asked from, where that is another place than it worked in.
    pub asked_from: Option<String>,
    /// The branch purlis cut for it, where its dispatch gave it one.
    pub branch: Option<String>,
    /// Its report's first line, cut short: what a row says before it is opened.
    pub says: String,
    /// The session record its chat wrote, by its project-relative path, where it wrote one.
    pub session_record: Option<String>,
    /// Whether Reopen is offered: its record names the conversation it ended in, it was not
    /// reopened already, and no Reopen of it is under way. The rest of the finished row's
    /// rules (the harness, the folder) are checked for the one row that is opened ([`read`]),
    /// and again by the press.
    pub reopens: bool,
    /// Whether it was reopened as an ordinary chat already.
    pub reopened: bool,
    /// Why the last Reopen of it did not hold, where one did not.
    pub not_reopened: Option<String>,
}

/// What the Past tasks view is handed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PastTasks {
    /// Newest ended first. Of a later read ([`PastSince`]), only the rows that are new or changed.
    pub rows: Vec<PastTask>,
    /// Tasks of this workspace that have reported and whose chat is still open, by record id:
    /// they are in the Chats list, and the next read asks about them by name.
    pub waiting: Vec<String>,
    /// Whether the store was read whole. A later read's rows are merged into the list; its
    /// counts below are not the store's, and the window keeps the whole read's.
    pub whole: bool,
    /// Past tasks older than the ones listed, left out for the bound.
    pub older: u32,
    /// The bound: the most rows a whole read answers.
    pub most: u32,
    /// Records in the project's store that could not be read: skipped and counted.
    pub unread: u32,
    /// Records purlis will not draw (`dispatchrecord::sound`).
    pub undrawn: u32,
    /// What the next read hands back as [`PastSince::at`].
    pub read_at: String,
}

/// What a later read of the list is asked with: the read it follows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
pub(crate) struct PastSince {
    /// [`PastTasks::read_at`] of the read before.
    pub at: String,
    /// [`PastTasks::waiting`] of the read before.
    pub also: Vec<String>,
}

/// One past task, opened: its brief and its report, **as written and drawn as text**.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PastTaskRead {
    pub id: String,
    /// The brief it started on.
    pub brief: String,
    /// Its report: the task's words, or purlis's for one that sent none.
    pub report: String,
    /// What the report says changed, in the task's words, where it said.
    pub changed: Option<String>,
    /// The files and the commits its report named, where it named any.
    pub files: Vec<String>,
    pub commits: Vec<String>,
    /// Why it cannot be reopened, where it cannot: the finished row's own sentence
    /// ([`crate::finished::reopening`]). `null` where a Reopen would be taken.
    pub cannot_reopen: Option<String>,
}

/// The most of a report's first line a row carries, in characters.
const MOST_SAID: usize = 200;

/// A place as a row says it: the workspace's name, or `project root`.
fn place(workspace: Option<&str>) -> String {
    workspace.unwrap_or("project root").to_owned()
}

/// A report's first line that says anything, held to [`MOST_SAID`].
fn first_line(report: &str) -> String {
    let line = report
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    match line.char_indices().nth(MOST_SAID) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

/// Why the past task `record` was reopened already, in the one sentence both the row and the
/// press say.
pub(crate) const REOPENED_ALREADY: &str =
    "It was reopened already: a task is reopened once, and that chat carries it on.";

/// `record` as its row, where it is a finished task's.
pub(crate) fn row(record: &Record, now: chrono::DateTime<chrono::Utc>) -> Option<PastTask> {
    let how = Finished::of(record)?;
    let worked = record.place.workspace.as_deref();
    let asked = record.asker.workspace.as_deref();
    Some(PastTask {
        id: record.id.clone(),
        name: record
            .task
            .clone()
            .unwrap_or_else(|| record.worker.chat.name.clone()),
        persona: record.persona.clone(),
        asker: record.asker.chat.name.clone(),
        asker_persona: record.asker.chat.persona.clone(),
        by_person: record.asker.by_person,
        how: how.into(),
        outcome: how.word().to_owned(),
        started: record.started.clone(),
        ended: record.ended.clone(),
        duration: crate::dispatches::lasted(record, now),
        place: place(worked),
        asked_from: (asked != worked).then(|| place(asked)),
        branch: record
            .place
            .worktree
            .as_ref()
            .and_then(|tree| tree.branch.clone()),
        says: record
            .report
            .as_ref()
            .map(|report| first_line(&report.text))
            .unwrap_or_default(),
        session_record: record.worker.session_record.clone(),
        reopens: record.conversation.is_some() && !record.reopened,
        reopened: record.reopened,
        not_reopened: None,
    })
}

/// The past tasks of `workspace` (`None` is the project's root) in `held`'s project: the
/// whole list, bounded, or what changed since the read `since` names.
pub(crate) fn listed(held: &Held, workspace: Option<&str>, since: Option<&PastSince>) -> PastTasks {
    let open = crate::dispatches::open_chats(held);
    let now = chrono::Utc::now();
    // A marker that does not read is no marker: the store is read whole.
    let at = since.and_then(|since| since.at.parse::<u64>().ok());
    let also: &[String] = match (at, since) {
        (Some(_), Some(since)) => &since.also,
        _ => &[],
    };
    let past = dispatchrecord::past(
        held.root(),
        workspace,
        |worker| open.iter().any(|one| crate::finished::is_open(worker, one)),
        &dispatchrecord::PastAsk {
            most: dispatchrecord::MOST_PAST,
            since: at,
            also,
        },
    );
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    PastTasks {
        rows: past
            .records
            .iter()
            .filter_map(|record| {
                let reopening = held.tasks().reopening(&record.id);
                let drawn = row(record, now)?;
                Some(PastTask {
                    reopens: drawn.reopens && !reopening,
                    not_reopened: held.tasks().not_reopened(&record.id),
                    ..drawn
                })
            })
            .collect(),
        waiting: past.waiting,
        whole: past.whole,
        older: count(past.older),
        most: count(dispatchrecord::MOST_PAST),
        unread: count(past.unread),
        undrawn: count(past.refused),
        read_at: past.read_at.to_string(),
    }
}

/// The past task `id`, opened: its brief, its report and what it said it changed, and why it
/// cannot be reopened where it cannot. An error for a record that is not there, is not one
/// purlis draws, or is not a finished task's.
pub(crate) fn read(held: &Held, id: &str) -> Result<PastTaskRead, String> {
    let record = dispatchrecord::read(held.root(), id)
        .filter(dispatchrecord::sound)
        .filter(|record| Finished::of(record).is_some())
        .ok_or_else(|| {
            "purlis no longer has a record of that task: it is collected 30 days after it was \
             last written."
                .to_owned()
        })?;
    let report = record.report.as_ref();
    let cannot_reopen = if record.reopened {
        Some(REOPENED_ALREADY.to_owned())
    } else {
        crate::finished::reopening(held, id).err()
    };
    Ok(PastTaskRead {
        id: record.id.clone(),
        brief: record.brief.clone(),
        report: report.map(|report| report.text.clone()).unwrap_or_default(),
        changed: report.and_then(|report| report.changed.said.clone()),
        files: report
            .map(|report| report.changed.files.clone())
            .unwrap_or_default(),
        commits: report
            .map(|report| report.changed.commits.clone())
            .unwrap_or_default(),
        cannot_reopen,
    })
}

/// **A workspace's past tasks** (#1510): every task asked from `workspace` or worked in it
/// whose dispatch has ended and whose chat is closed, newest first, from the records this
/// machine keeps for this project. `workspace` is `null` for the project's root. With `since`,
/// only what was written since that read, and what was waiting then. On a blocking thread.
#[tauri::command]
#[specta::specta]
pub(crate) async fn past_tasks(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    workspace: Option<String>,
    since: Option<PastSince>,
) -> Result<PastTasks, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading the past tasks", move || {
        Ok(listed(&held, workspace.as_deref(), since.as_ref()))
    })
    .await
}

/// One past task's brief and report (#1510), read when its row is opened: text, and drawn as
/// text. On a blocking thread.
#[tauri::command]
#[specta::specta]
pub(crate) async fn past_task(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    id: String,
) -> Result<PastTaskRead, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading a past task", move || read(&held, &id)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_s_first_line_is_the_first_that_says_anything_and_is_cut_at_a_character() {
        assert_eq!(first_line("\n  \nHealthy.\nThree of three."), "Healthy.");
        assert_eq!(first_line(""), "");
        let long = "é".repeat(MOST_SAID + 5);
        let said = first_line(&long);
        assert_eq!(said.chars().count(), MOST_SAID + 1);
        assert!(said.ends_with('…'));
    }
}
