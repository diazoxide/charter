//! What a session's tasks used (#1500, V100-43): the tokens of each chat of a tab, and one
//! total line at the foot of the tab's menu (`5 tasks · 310k tokens · 6m`). **No money.**
//!
//! # One source for a chat's tokens
//!
//! A chat's tokens are what its harness said its conversation has cost in all
//! (`purlis_core::usage::spent`), the figure the Dispatches tab says too: nothing here counts
//! a token itself. An open chat is read from its conversation's file as the window asks; a task
//! that has ended keeps what its record kept when it ended (`dispatchrecord::Record::usage`),
//! so what it used does not move after its end and survives a restart.
//!
//! **A harness that says nothing has a dash, never a zero** (V100-71): a figure is `None` where
//! nothing was said, the window draws `—` for it, and the total says it counts only what was
//! said ("at least 310k tokens"). A harness's own helpers are inside its figure, as its harness
//! counts them; they are never added a second time.
//!
//! # When it is read
//!
//! Only when the window asks: as the tab's menu opens and when a chat of it changes state while
//! it is open, and as the pointer comes onto a task's row. Nothing polls.
//!
//! **A reported figure, which a chat can alter** (D-1452-12): it is shown as said, and nothing
//! decides anything by it.

use std::path::Path;

use purlis_core::dispatchrecord::{self, Record};
use purlis_core::usage::{self, Spent};

use crate::planes::{Held, PlaneId, Planes};

/// One open chat's tokens, by its number.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct ChatUsed {
    pub session: u32,
    /// Its tokens in and out as its harness counted them (`15k in, 4k out`); `null` where its
    /// harness said none.
    pub tokens: Option<String>,
}

/// One ended task's tokens, by its dispatch record's id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct FinishedUsed {
    pub id: String,
    /// What it used, as kept when it ended; `null` where its harness said none.
    pub tokens: Option<String>,
}

/// The menu's total line, and what it adds up.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct UsedTotal {
    /// How many tasks it counts.
    pub tasks: u32,
    /// The line as drawn: `5 tasks · 310k tokens · 6m`.
    pub said: String,
    /// What the line adds up, in a sentence: its title.
    pub explained: String,
}

/// What the window is answered for one ask.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct TasksUsed {
    pub chats: Vec<ChatUsed>,
    pub finished: Vec<FinishedUsed>,
    /// `null` where the ask named no task.
    pub total: Option<UsedTotal>,
}

/// What one chat's figures are, as read: its tokens, and for a task how long it has worked.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Figure {
    pub spent: Option<Spent>,
    /// Seconds it worked, from its dispatch to its end or to now.
    pub ran: Option<i64>,
}

/// Its tokens in and out, added: `None` where neither was said.
fn tokens_of(spent: Option<&Spent>) -> Option<u64> {
    let spent = spent?;
    match (spent.input_tokens, spent.output_tokens) {
        (None, None) => None,
        (input, output) => Some(input.unwrap_or(0).saturating_add(output.unwrap_or(0))),
    }
}

/// A chat's tokens as a row says them, or `None` where its harness said none.
fn line_of(spent: Option<&Spent>) -> Option<String> {
    let spent = spent?;
    crate::dispatches::counted(spent.input_tokens, spent.output_tokens)
}

/// A count of tokens as purlis spells it everywhere (`310k`, `1.2M`).
fn spelled(n: u64) -> String {
    usage::tokens(i64::try_from(n).unwrap_or(i64::MAX))
}

/// A sum of working time, said coarsely: `45s`, `6m`, `2h 5m`.
fn time_said(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes) = (seconds / 3600, seconds % 3600 / 60);
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{seconds}s")
    }
}

/// **The menu's total**: the session's own chat (`own`) and every task, open (`chats`) and
/// ended (`finished`). Its tokens are the sum of the lines' own figures, so the total is never
/// a number the lines do not add up to; its time is the tasks' own times, added. `None` where
/// there is no task to total.
pub(crate) fn total(own: Option<Figure>, tasks: &[Figure]) -> Option<UsedTotal> {
    if tasks.is_empty() {
        return None;
    }
    let count = u32::try_from(tasks.len()).unwrap_or(u32::MAX);
    let own_tokens = own.as_ref().and_then(|own| tokens_of(own.spent.as_ref()));
    let task_tokens: Vec<Option<u64>> = tasks
        .iter()
        .map(|task| tokens_of(task.spent.as_ref()))
        .collect();
    let said_by_tasks: Option<u64> = task_tokens
        .iter()
        .flatten()
        .copied()
        .reduce(u64::saturating_add);
    let unreported = task_tokens.iter().filter(|one| one.is_none()).count()
        + usize::from(own.is_some() && own_tokens.is_none());
    let all: Option<u64> = [own_tokens, said_by_tasks]
        .into_iter()
        .flatten()
        .reduce(u64::saturating_add);
    let ran: Option<i64> = tasks
        .iter()
        .filter_map(|task| task.ran)
        .reduce(i64::saturating_add);

    let tasks_said = if count == 1 {
        "1 task".to_owned()
    } else {
        format!("{count} tasks")
    };
    let tokens_said = match all {
        None => "— tokens".to_owned(),
        Some(n) if unreported > 0 => format!("at least {} tokens", spelled(n)),
        Some(n) => format!("{} tokens", spelled(n)),
    };
    let said = [Some(tasks_said), Some(tokens_said), ran.map(time_said)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");

    let or_dash = |n: Option<u64>| n.map_or_else(|| "—".to_owned(), spelled);
    let mut explained = Vec::new();
    if all.is_none() {
        explained.push("No harness here reported its tokens.".to_owned());
    } else if own.is_some() {
        explained.push(format!(
            "Tokens, as each harness reported them: {} by the session's own chat, {} by its tasks.",
            or_dash(own_tokens),
            or_dash(said_by_tasks),
        ));
    } else {
        explained.push(format!(
            "Tokens, as each harness reported them: {} by the tasks.",
            or_dash(said_by_tasks),
        ));
    }
    if unreported > 0 && all.is_some() {
        explained.push(format!(
            "{unreported} {} reported none, so the total counts only what was reported.",
            if unreported == 1 {
                "chat's harness"
            } else {
                "chats' harnesses"
            }
        ));
    }
    if let Some(ran) = ran {
        explained.push(format!(
            "{}: how long the tasks worked, added up.",
            time_said(ran)
        ));
    }
    Some(UsedTotal {
        tasks: count,
        said,
        explained: explained.join(" "),
    })
}

/// A record's time as it keeps it, read.
fn at(stamp: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|read| read.with_timezone(&chrono::Utc))
}

/// How long `record`'s task worked: to its end, or to `now` while it works.
pub(crate) fn ran(record: &Record, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    ran_between(&record.started, record.ended.as_deref(), now)
}

/// How long from `started` to `ended`, or to `now` where it has not ended.
fn ran_between(
    started: &str,
    ended: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<i64> {
    let started = at(started)?;
    let ended = match ended {
        Some(ended) => at(ended)?,
        None => now,
    };
    Some((ended - started).num_seconds().max(0))
}

/// A task that has ended: what its record kept.
pub(crate) fn of_record(record: &Record, now: chrono::DateTime<chrono::Utc>) -> Figure {
    Figure {
        spent: record.usage.filter(|spent| !spent.is_empty()),
        ran: ran(record, now),
    }
}

/// Every figure of one ask, read from the project at `root`. `conversation` and `chat` say what
/// the app knows of an open chat: the conversation it is in, and how a record names it.
pub(crate) fn read(
    root: &Path,
    own: Option<u32>,
    chats: &[u32],
    finished: &[String],
    conversation: impl Fn(u32) -> Option<String>,
    chat: impl Fn(u32) -> Option<dispatchrecord::ChatRef>,
    now: chrono::DateTime<chrono::Utc>,
) -> TasksUsed {
    let spent_of = |session: u32| {
        conversation(session).and_then(|conversation| usage::spent(root, &conversation))
    };
    // One read of the store for every open task's start, however many there are.
    let running: Vec<Record> = if chats.is_empty() {
        Vec::new()
    } else {
        dispatchrecord::list(root)
            .into_iter()
            .filter(Record::running)
            .collect()
    };
    let open: Vec<(u32, Figure)> = chats
        .iter()
        .map(|&session| {
            let started = chat(session).and_then(|me| {
                running
                    .iter()
                    .find(|record| dispatchrecord::same_chat(&record.worker.chat, &me))
            });
            (
                session,
                Figure {
                    spent: spent_of(session),
                    ran: started.and_then(|record| ran(record, now)),
                },
            )
        })
        .collect();
    let ended: Vec<(String, Figure)> = finished
        .iter()
        .map(|id| {
            let figure = dispatchrecord::read(root, id)
                .map(|record| of_record(&record, now))
                .unwrap_or_default();
            (id.clone(), figure)
        })
        .collect();
    let own = own.map(|session| {
        (
            session,
            Figure {
                spent: spent_of(session),
                ran: None,
            },
        )
    });
    let tasks: Vec<Figure> = open
        .iter()
        .map(|(_, figure)| *figure)
        .chain(ended.iter().map(|(_, figure)| *figure))
        .collect();
    TasksUsed {
        total: total(own.map(|(_, figure)| figure), &tasks),
        chats: own
            .into_iter()
            .chain(open.iter().copied())
            .map(|(session, figure)| ChatUsed {
                session,
                tokens: line_of(figure.spent.as_ref()),
            })
            .collect(),
        finished: ended
            .iter()
            .map(|(id, figure)| FinishedUsed {
                id: id.clone(),
                tokens: line_of(figure.spent.as_ref()),
            })
            .collect(),
    }
}

/// The most chats one ask reads: a tab of fifty chats is well inside it, and an ask past it is
/// not one the window makes.
const MOST: usize = 256;

/// **What the chats of a tab used** (#1500): `own` is the session's own chat, counted in the
/// tokens and not as a task; `chats` its open tasks, and any it still shows that has ended;
/// `finished` the dispatch records of its ended tasks. Asked as the tab's menu opens, and as
/// the pointer comes onto a task's row, never on a timer. On a blocking thread, as it reads
/// the dispatch records.
#[tauri::command]
#[specta::specta]
pub(crate) async fn tasks_used(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    own: Option<u32>,
    chats: Vec<u32>,
    finished: Vec<String>,
) -> Result<TasksUsed, String> {
    if chats.len() > MOST || finished.len() > MOST {
        return Err("Too many chats in one ask.".to_owned());
    }
    let held = planes.held(&plane)?;
    crate::off_the_window("reading what a tab's tasks used", move || {
        Ok(of_held(&held, own, &chats, &finished))
    })
    .await
}

/// [`read`], with what the app knows of its open chats.
fn of_held(held: &Held, own: Option<u32>, chats: &[u32], finished: &[String]) -> TasksUsed {
    read(
        held.root(),
        own,
        chats,
        finished,
        |session| crate::dispatches::conversation_of(held, session),
        |session| crate::dispatches::chat_ref(held, session),
        chrono::Utc::now(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(input: u64, output: u64) -> Figure {
        Figure {
            spent: Some(Spent {
                input_tokens: Some(input),
                output_tokens: Some(output),
                cost_usd: Some(1.25),
            }),
            ran: None,
        }
    }

    fn worked(mut figure: Figure, seconds: i64) -> Figure {
        figure.ran = Some(seconds);
        figure
    }

    const SILENT: Figure = Figure {
        spent: None,
        ran: None,
    };

    #[test]
    fn the_total_is_the_sum_of_the_figures_its_lines_show() {
        let own = said(100_000, 20_000);
        let tasks = [
            worked(said(60_000, 10_000), 120),
            worked(said(90_000, 30_000), 240),
        ];

        let total = total(Some(own), &tasks).expect("a total");

        // 120k + 70k + 120k: the session's own chat and both tasks, as each line says them.
        assert_eq!(total.said, "2 tasks · 310k tokens · 6m");
        assert_eq!(total.tasks, 2);
        assert!(
            total
                .explained
                .contains("120k by the session's own chat, 190k by its tasks"),
            "{}",
            total.explained
        );
        assert!(!total.said.contains('$') && !total.explained.contains('$'));
    }

    #[test]
    fn a_harness_that_reports_nothing_is_a_dash_and_the_total_says_it_counts_only_what_was_said() {
        let tasks = [worked(said(200_000, 10_000), 30), worked(SILENT, 30)];

        let total = total(None, &tasks).expect("a total");

        assert_eq!(total.said, "2 tasks · at least 210k tokens · 1m");
        assert!(total.explained.contains("1 chat's harness reported none"));
        assert_eq!(line_of(None), None);
    }

    #[test]
    fn nothing_reported_anywhere_is_a_dash_and_never_a_zero() {
        let total = total(Some(SILENT), &[SILENT]).expect("a total");

        assert_eq!(total.said, "1 task · — tokens");
        assert!(!total.said.contains('0'));
    }

    #[test]
    fn a_tab_with_no_task_has_no_total() {
        assert_eq!(total(Some(said(1, 1)), &[]), None);
    }

    #[test]
    fn a_line_says_its_tokens_in_and_out_and_never_its_cost() {
        let figure = said(15_234, 4_521);
        assert_eq!(
            line_of(figure.spent.as_ref()).as_deref(),
            Some("15k in, 4k out")
        );
        let only_cost = Spent {
            cost_usd: Some(0.5),
            ..Spent::default()
        };
        assert_eq!(line_of(Some(&only_cost)), None);
        assert_eq!(tokens_of(Some(&only_cost)), None);
    }

    #[test]
    fn time_is_said_coarsely() {
        assert_eq!(time_said(45), "45s");
        assert_eq!(time_said(6 * 60 + 59), "6m");
        assert_eq!(time_said(2 * 3600 + 5 * 60), "2h 5m");
    }

    #[test]
    fn an_ended_task_keeps_what_its_record_kept_and_an_open_one_is_read_from_its_conversation() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        let conversation = "11111111-2222-4333-8444-555555555555";
        assert!(usage::record_spend(
            &root,
            &serde_json::json!({
                "session_id": conversation,
                "context_window": { "total_input_tokens": 50_000, "total_output_tokens": 2_000 },
            })
        ));

        let used = read(
            &root,
            Some(1),
            &[2, 3],
            &["no-such-record".to_owned()],
            |session| (session == 2).then(|| conversation.to_owned()),
            |_| None,
            chrono::Utc::now(),
        );

        assert_eq!(
            used.chats,
            vec![
                ChatUsed {
                    session: 1,
                    tokens: None
                },
                ChatUsed {
                    session: 2,
                    tokens: Some("50k in, 2k out".to_owned())
                },
                ChatUsed {
                    session: 3,
                    tokens: None
                },
            ]
        );
        assert_eq!(
            used.finished,
            vec![FinishedUsed {
                id: "no-such-record".to_owned(),
                tokens: None
            }]
        );
        assert_eq!(
            used.total.expect("a total").said,
            "3 tasks · at least 52k tokens"
        );
    }

    #[test]
    fn a_task_that_ended_worked_from_its_dispatch_to_its_end() {
        let now = at("2026-10-08T11:00:00Z").expect("a time");
        let started = "2026-10-08T10:00:00Z";
        assert_eq!(
            ran_between(started, Some("2026-10-08T10:06:30Z"), now),
            Some(390)
        );
        assert_eq!(ran_between(started, None, now), Some(3600));
        assert_eq!(ran_between("not a time", None, now), None);
    }
}
