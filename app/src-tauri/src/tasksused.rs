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
//! **The total is the tasks' own** (the ticket's acceptance line): the sum of the figures its
//! task lines show. The session's own chat is said beside it in the title and is not in it.
//!
//! **Nothing is guessed** (V100-71). A figure that is missing is a dash, never a zero, and says
//! why ([`Unsaid`]): nothing reported yet, nothing reported, or not known. A total that misses
//! any task's figure, or has only half of one, says `at least`; so does a time that misses any
//! task's time. A harness's own helpers are inside its figure, as its harness counts them; they
//! are never added a second time.
//!
//! # When it is read, and what one read costs
//!
//! Only when the window asks: as the tab's menu opens and when a chat of it changes state while
//! it is open, and as the pointer comes to rest on a task's row. Nothing polls. **A hover asks
//! for its one figure and nothing else** ([`Scope::Hover`]): one usage file or one record, no
//! time and no total. The menu's time needs each open task's record, which is found once per
//! chat and then kept by its id ([`Starts`]), so no read walks the whole store again.
//!
//! **A reported figure, which a chat can alter** (D-1452-12): it is shown as said, and nothing
//! decides anything by it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use purlis_core::dispatchrecord::{self, ChatRef, Record};
use purlis_core::usage::{self, Spent};

use crate::planes::{Held, PlaneId, Planes};

/// Why a chat's tokens are a dash: true in every case that leads to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Unsaid {
    /// An open chat whose conversation is known and whose harness has said nothing so far:
    /// no turn has ended yet, or its harness reports none.
    NotYet,
    /// A task that ended with no figure kept: its harness said nothing.
    Nothing,
    /// purlis cannot tell: it does not know the chat's conversation, or the record could not
    /// be read.
    NotKnown,
}

/// One open chat's tokens, by its number.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct ChatUsed {
    pub session: u32,
    /// Its tokens in and out as its harness counted them (`15k in, 4k out`); `null` where none.
    pub tokens: Option<String>,
    /// Why there are none, where there are none.
    pub unsaid: Option<Unsaid>,
}

/// One ended task's tokens, by its dispatch record's id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct FinishedUsed {
    pub id: String,
    /// What it used, as kept when it ended; `null` where none.
    pub tokens: Option<String>,
    pub unsaid: Option<Unsaid>,
}

/// The menu's total line, and what it adds up.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct UsedTotal {
    /// How many tasks it counts.
    pub tasks: u32,
    /// The line as drawn: `5 tasks · 310k tokens · 6m`.
    pub said: String,
    /// What the line adds up and what it leaves out, in sentences: its title.
    pub explained: String,
}

/// What the window is answered for one ask.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct TasksUsed {
    pub chats: Vec<ChatUsed>,
    pub finished: Vec<FinishedUsed>,
    /// `null` for a hover's ask, and where the ask named no task.
    pub total: Option<UsedTotal>,
}

/// What an ask is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    /// A row's hover: its figure only. No time is looked for and no total is made.
    Hover,
    /// A tab's menu: every line's figure, each task's time, and the total.
    Menu,
}

/// What one chat's tokens are, as read.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum Tokens {
    /// What its harness said.
    Said(Spent),
    /// Its source was read and holds nothing.
    Unsaid(Unsaid),
    /// Its source could not be read or named.
    #[default]
    NotKnown,
}

/// What one chat's figures are, as read: its tokens, and for a task how long it has worked.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Figure {
    pub tokens: Tokens,
    /// Seconds it worked, from its dispatch to its end or to now; `None` where not known.
    pub ran: Option<i64>,
}

impl Figure {
    /// Its tokens added, and whether that is the whole figure: `None` where nothing was said,
    /// and `false` where only one side of it was.
    fn counted(&self) -> Option<(u64, bool)> {
        let Tokens::Said(spent) = self.tokens else {
            return None;
        };
        match (spent.input_tokens, spent.output_tokens) {
            (None, None) => None,
            (Some(input), Some(output)) => Some((input.saturating_add(output), true)),
            (one, other) => Some((one.or(other).unwrap_or(0), false)),
        }
    }

    /// Its tokens as a row says them (`15k in, 4k out`), or why there are none.
    fn line(&self) -> (Option<String>, Option<Unsaid>) {
        match self.tokens {
            Tokens::Said(spent) => {
                match crate::dispatches::counted(spent.input_tokens, spent.output_tokens) {
                    Some(line) => (Some(line), None),
                    // A cost and no tokens is no token figure.
                    None => (None, Some(Unsaid::Nothing)),
                }
            }
            Tokens::Unsaid(why) => (None, Some(why)),
            Tokens::NotKnown => (None, Some(Unsaid::NotKnown)),
        }
    }
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

/// `1 task` or `n tasks`.
fn tasks_said(n: usize) -> String {
    if n == 1 {
        "1 task".to_owned()
    } else {
        format!("{n} tasks")
    }
}

/// The session's own chat, as the title says it beside the total.
fn own_said(own: &Figure) -> String {
    match (own.counted(), own.line().1) {
        (Some((n, true)), _) => spelled(n),
        (Some((n, false)), _) => format!("{} (only part reported)", spelled(n)),
        (None, Some(Unsaid::NotYet)) => "— (nothing reported yet)".to_owned(),
        (None, Some(Unsaid::Nothing)) => "— (nothing reported)".to_owned(),
        (None, _) => "— (not known)".to_owned(),
    }
}

/// **The menu's total**: the tasks' own figures, open (`chats`) and ended (`finished`), and
/// the session's own chat (`own`) said beside it in the title, not in it. Its tokens are the
/// sum of what the task lines show; its time is the tasks' own times, added. Either says
/// `at least` where any task's part of it is missing. `None` where there is no task.
pub(crate) fn total(own: Option<Figure>, tasks: &[Figure]) -> Option<UsedTotal> {
    if tasks.is_empty() {
        return None;
    }
    let counted: Vec<Option<(u64, bool)>> = tasks.iter().map(Figure::counted).collect();
    let tokens: Option<u64> = counted
        .iter()
        .flatten()
        .map(|(n, _)| *n)
        .reduce(u64::saturating_add);
    let missing = counted.iter().filter(|one| one.is_none()).count();
    let halves = counted
        .iter()
        .filter(|one| matches!(one, Some((_, false))))
        .count();
    let ran: Option<i64> = tasks
        .iter()
        .filter_map(|task| task.ran)
        .reduce(i64::saturating_add);
    let untimed = tasks.iter().filter(|task| task.ran.is_none()).count();

    let tokens_said = match tokens {
        None => "— tokens".to_owned(),
        Some(n) if missing + halves > 0 => format!("at least {} tokens", spelled(n)),
        Some(n) => format!("{} tokens", spelled(n)),
    };
    let time = ran.map(|ran| {
        if untimed > 0 {
            format!("at least {}", time_said(ran))
        } else {
            time_said(ran)
        }
    });
    let said = [Some(tasks_said(tasks.len())), Some(tokens_said), time]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");

    let mut explained = Vec::new();
    match tokens {
        None => explained.push("No task has a token figure yet.".to_owned()),
        Some(n) => explained.push(format!(
            "Tokens the tasks used, as each harness reported them: {}.",
            spelled(n)
        )),
    }
    if missing > 0 && tokens.is_some() {
        explained.push(format!(
            "Left out: {} with no figure (nothing reported, or not known).",
            tasks_said(missing)
        ));
    }
    if halves > 0 {
        explained.push(format!(
            "Left out: half of the figure of {}, whose harness reported only tokens in or only \
             tokens out.",
            tasks_said(halves)
        ));
    }
    match ran {
        Some(ran) => {
            explained.push(format!(
                "{}: how long the tasks worked, added up.",
                time_said(ran)
            ));
            if untimed > 0 {
                explained.push(format!(
                    "Left out: the time of {}, which is not known.",
                    tasks_said(untimed)
                ));
            }
        }
        None => explained.push("How long the tasks worked is not known.".to_owned()),
    }
    if let Some(own) = own {
        explained.push(format!(
            "The session's own chat: {}, not in this total.",
            own_said(&own)
        ));
    }
    Some(UsedTotal {
        tasks: u32::try_from(tasks.len()).unwrap_or(u32::MAX),
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

/// A task that has ended: what its record kept, or not known where it could not be read.
pub(crate) fn of_record(record: Option<&Record>, now: chrono::DateTime<chrono::Utc>) -> Figure {
    let Some(record) = record else {
        return Figure::default();
    };
    Figure {
        tokens: match record.usage {
            Some(spent) if !spent.is_empty() => Tokens::Said(spent),
            _ => Tokens::Unsaid(Unsaid::Nothing),
        },
        ran: ran(record, now),
    }
}

/// **Which record is each open task's** (fold-in of the #1500 review): the newest dispatch of
/// a chat, by the chat's id, found once with one walk of the store and then read by its id, so
/// the menu's time costs one small read per task. A record that no longer reads, or no longer
/// names the chat, is looked for again.
#[derive(Default)]
pub(crate) struct Starts(Mutex<HashMap<(PathBuf, String), String>>);

impl Starts {
    /// The newest record of chat `me` in the project at `root`.
    fn of(&self, root: &Path, me: &ChatRef) -> Option<Record> {
        // A chat with no id is named only by a number a launch deals again: never kept.
        let Some(id) = me.id.clone() else {
            return dispatchrecord::latest_for(root, me);
        };
        let key = (root.to_path_buf(), id);
        let kept = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
            .cloned();
        if let Some(record) = kept
            .and_then(|record| dispatchrecord::read(root, &record))
            .filter(|record| dispatchrecord::same_chat(&record.worker.chat, me))
        {
            return Some(record);
        }
        let found = dispatchrecord::latest_for(root, me)?;
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, found.id.clone());
        Some(found)
    }
}

/// The one [`Starts`] of this app.
fn starts() -> &'static Starts {
    static STARTS: std::sync::OnceLock<Starts> = std::sync::OnceLock::new();
    STARTS.get_or_init(Starts::default)
}

/// What the app knows of its open chats, for [`read`].
pub(crate) struct Known<'a, C, R> {
    /// The conversation an open chat is in.
    pub conversation: C,
    /// How a record names an open chat.
    pub chat: R,
    /// Which record is each open task's.
    pub records: &'a Starts,
}

/// Every figure of one ask, read from the project at `root`.
pub(crate) fn read<C, R>(
    root: &Path,
    scope: Scope,
    own: Option<u32>,
    chats: &[u32],
    finished: &[String],
    known: &Known<'_, C, R>,
    now: chrono::DateTime<chrono::Utc>,
) -> TasksUsed
where
    C: Fn(u32) -> Option<String>,
    R: Fn(u32) -> Option<ChatRef>,
{
    let tokens_of = |session: u32| match (known.conversation)(session) {
        None => Tokens::NotKnown,
        Some(conversation) => {
            usage::spent(root, &conversation).map_or(Tokens::Unsaid(Unsaid::NotYet), Tokens::Said)
        }
    };
    let open: Vec<(u32, Figure)> = chats
        .iter()
        .map(|&session| {
            let ran = match scope {
                Scope::Hover => None,
                Scope::Menu => (known.chat)(session)
                    .and_then(|me| known.records.of(root, &me))
                    .and_then(|record| ran(&record, now)),
            };
            (
                session,
                Figure {
                    tokens: tokens_of(session),
                    ran,
                },
            )
        })
        .collect();
    let ended: Vec<(String, Figure)> = finished
        .iter()
        .map(|id| {
            let record = dispatchrecord::read(root, id);
            (id.clone(), of_record(record.as_ref(), now))
        })
        .collect();
    let own = own.map(|session| {
        (
            session,
            Figure {
                tokens: tokens_of(session),
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
        total: match scope {
            Scope::Hover => None,
            Scope::Menu => total(own.map(|(_, figure)| figure), &tasks),
        },
        chats: own
            .into_iter()
            .chain(open.iter().copied())
            .map(|(session, figure)| {
                let (tokens, unsaid) = figure.line();
                ChatUsed {
                    session,
                    tokens,
                    unsaid,
                }
            })
            .collect(),
        finished: ended
            .iter()
            .map(|(id, figure)| {
                let (tokens, unsaid) = figure.line();
                FinishedUsed {
                    id: id.clone(),
                    tokens,
                    unsaid,
                }
            })
            .collect(),
    }
}

/// The most chats one ask reads: a tab of fifty chats is well inside it, and an ask past it is
/// not one the window makes.
const MOST: usize = 256;

/// **What the chats of a tab used** (#1500). `scope` says what for: a row's hover reads its
/// one figure; a tab's menu reads every line, each task's time and the total. `own` is the
/// session's own chat, said beside the total and not in it; `chats` its open tasks, and any it
/// still shows that has ended; `finished` the dispatch records of its ended tasks. Never on a
/// timer. On a blocking thread, as it reads files.
#[tauri::command]
#[specta::specta]
pub(crate) async fn tasks_used(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    scope: Scope,
    own: Option<u32>,
    chats: Vec<u32>,
    finished: Vec<String>,
) -> Result<TasksUsed, String> {
    if chats.len() > MOST || finished.len() > MOST {
        return Err("Too many chats in one ask.".to_owned());
    }
    let held = planes.held(&plane)?;
    crate::off_the_window("reading what a tab's tasks used", move || {
        Ok(of_held(&held, scope, own, &chats, &finished))
    })
    .await
}

/// [`read`], with what the app knows of its open chats.
fn of_held(
    held: &Held,
    scope: Scope,
    own: Option<u32>,
    chats: &[u32],
    finished: &[String],
) -> TasksUsed {
    let known = Known {
        conversation: |session| crate::dispatches::conversation_of(held, session),
        chat: |session| crate::dispatches::chat_ref(held, session),
        records: starts(),
    };
    read(
        held.root(),
        scope,
        own,
        chats,
        finished,
        &known,
        chrono::Utc::now(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(input: u64, output: u64) -> Figure {
        Figure {
            tokens: Tokens::Said(Spent {
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
        tokens: Tokens::Unsaid(Unsaid::NotYet),
        ran: None,
    };

    #[test]
    fn the_total_is_the_sum_of_the_tasks_own_figures_and_says_the_session_beside_it() {
        let own = said(100_000, 20_000);
        let tasks = [
            worked(said(60_000, 10_000), 120),
            worked(said(90_000, 30_000), 240),
        ];

        let total = total(Some(own), &tasks).expect("a total");

        // 70k + 120k: the tasks' own figures, as their lines say them. Not the session's 120k.
        assert_eq!(total.said, "2 tasks · 190k tokens · 6m");
        assert_eq!(total.tasks, 2);
        assert!(
            total
                .explained
                .contains("The session's own chat: 120k, not in this total."),
            "{}",
            total.explained
        );
        assert!(!total.explained.contains("Left out"), "{}", total.explained);
        assert!(!total.said.contains('$') && !total.explained.contains('$'));
    }

    #[test]
    fn a_task_with_no_figure_makes_the_total_at_least_and_never_a_zero() {
        let tasks = [worked(said(200_000, 10_000), 30), worked(SILENT, 30)];

        let total = total(None, &tasks).expect("a total");

        assert_eq!(total.said, "2 tasks · at least 210k tokens · 1m");
        assert!(total.explained.contains("Left out: 1 task with no figure"));
    }

    #[test]
    fn a_figure_with_only_one_side_counts_as_partial() {
        let half = Figure {
            tokens: Tokens::Said(Spent {
                input_tokens: Some(40_000),
                ..Spent::default()
            }),
            ran: Some(60),
        };

        let total = total(None, &[worked(said(10_000, 0), 60), half]).expect("a total");

        assert_eq!(total.said, "2 tasks · at least 50k tokens · 2m");
        assert!(total.explained.contains("half of the figure of 1 task"));
        assert_eq!(half.line().0.as_deref(), Some("40k in"));
    }

    #[test]
    fn a_task_whose_time_is_not_known_makes_the_time_at_least_and_the_title_says_so() {
        let total =
            total(None, &[worked(said(1_000, 1_000), 400), said(1_000, 1_000)]).expect("a total");

        assert_eq!(total.said, "2 tasks · 4k tokens · at least 6m");
        assert!(
            total
                .explained
                .contains("Left out: the time of 1 task, which is not known."),
            "{}",
            total.explained
        );
    }

    #[test]
    fn nothing_reported_anywhere_is_a_dash_and_never_a_zero() {
        let total = total(Some(SILENT), &[SILENT]).expect("a total");

        assert_eq!(total.said, "1 task · — tokens");
        assert!(!total.said.contains('0'));
        assert!(total.explained.contains("nothing reported yet"));
    }

    #[test]
    fn a_tab_with_no_task_has_no_total() {
        assert_eq!(total(Some(said(1, 1)), &[]), None);
    }

    #[test]
    fn a_line_says_its_tokens_in_and_out_and_never_its_cost() {
        assert_eq!(
            said(15_234, 4_521).line(),
            (Some("15k in, 4k out".to_owned()), None)
        );
        let only_cost = Figure {
            tokens: Tokens::Said(Spent {
                cost_usd: Some(0.5),
                ..Spent::default()
            }),
            ran: None,
        };
        assert_eq!(only_cost.line(), (None, Some(Unsaid::Nothing)));
        assert_eq!(only_cost.counted(), None);
    }

    #[test]
    fn time_is_said_coarsely() {
        assert_eq!(time_said(45), "45s");
        assert_eq!(time_said(6 * 60 + 59), "6m");
        assert_eq!(time_said(2 * 3600 + 5 * 60), "2h 5m");
    }

    const CONVERSATION: &str = "11111111-2222-4333-8444-555555555555";

    fn project_with_a_spend() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        assert!(usage::record_spend(
            &root,
            &serde_json::json!({
                "session_id": CONVERSATION,
                "context_window": { "total_input_tokens": 50_000, "total_output_tokens": 2_000 },
            })
        ));
        (dir, root)
    }

    #[test]
    fn each_dash_says_why_truly_no_turn_yet_unknown_conversation_unreadable_record() {
        let (_dir, root) = project_with_a_spend();
        let known = Known {
            // Chat 2 has a figure; chat 3's conversation is known and holds none; chat 1's
            // conversation is not known.
            conversation: |session: u32| match session {
                2 => Some(CONVERSATION.to_owned()),
                3 => Some("22222222-2222-4333-8444-555555555555".to_owned()),
                _ => None,
            },
            chat: |_: u32| None,
            records: &Starts::default(),
        };

        let used = read(
            &root,
            Scope::Menu,
            Some(1),
            &[2, 3],
            &["no-such-record".to_owned()],
            &known,
            chrono::Utc::now(),
        );

        let line = |session: u32| {
            let one = used.chats.iter().find(|one| one.session == session);
            one.map(|one| (one.tokens.clone(), one.unsaid))
        };
        assert_eq!(line(1), Some((None, Some(Unsaid::NotKnown))));
        assert_eq!(line(2), Some((Some("50k in, 2k out".to_owned()), None)));
        assert_eq!(line(3), Some((None, Some(Unsaid::NotYet))));
        assert_eq!(
            used.finished,
            vec![FinishedUsed {
                id: "no-such-record".to_owned(),
                tokens: None,
                unsaid: Some(Unsaid::NotKnown),
            }]
        );
        let total = used.total.expect("a total");
        // The session's own chat is not in it, and no time is known.
        assert_eq!(total.said, "3 tasks · at least 52k tokens");
        assert!(total.explained.contains("— (not known), not in this total"));
    }

    #[test]
    fn a_hover_reads_its_one_figure_and_no_time_and_no_total() {
        let (_dir, root) = project_with_a_spend();
        let looked = std::cell::Cell::new(0);
        let known = Known {
            conversation: |_: u32| Some(CONVERSATION.to_owned()),
            // Asked only to find a record for the time: a hover never asks.
            chat: |_: u32| {
                looked.set(looked.get() + 1);
                None
            },
            records: &Starts::default(),
        };

        let used = read(
            &root,
            Scope::Hover,
            None,
            &[2],
            &[],
            &known,
            chrono::Utc::now(),
        );

        assert_eq!(used.total, None);
        assert_eq!(used.chats[0].tokens.as_deref(), Some("50k in, 2k out"));
        assert_eq!(looked.get(), 0);
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
