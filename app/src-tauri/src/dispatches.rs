//! The app's half of the dispatch record (#1452): writing one for each dispatch it starts, and
//! handing the window the list its Dispatches tab draws.
//!
//! `purlis_core::dispatchrecord` is the store and argues where it lives. This module is the
//! **only caller of its writers**, and every fact it writes comes from the app's own record of
//! the two chats: [`opened`] where the app starts a persona chat, [`needed_you`] where the
//! board puts that chat in the needs-you queue, [`reported`] where the app accepts its report,
//! [`ended`] where the app closes it, and [`session_recorded`] where the app writes a chat's
//! session record. Nothing on the hook channel names a record, so no line a chat sends makes
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

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use purlis_core::dispatchrecord::{self, ChatRef, Ending, Event, Opening, Outcome, Record};

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
    match cwd.strip_prefix(root) {
        Ok(inside) if inside.as_os_str().is_empty() => ".".to_owned(),
        Ok(inside) => inside.to_string_lossy().into_owned(),
        Err(_) => cwd.to_string_lossy().into_owned(),
    }
}

/// The app started a persona chat: its dispatch has a record from now. A record that could not
/// be written is said in the app's log and costs the dispatch nothing: the chat is open.
pub(crate) fn opened(held: &Held, opening: Opening) {
    if let Err(why) = dispatchrecord::open(held.root(), opening, chrono::Utc::now()) {
        tracing::warn!("purlis: a dispatch's record was not written ({why})");
    }
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

/// The app accepted chat `session`'s report: its dispatch ends with it. `outcome` and `text`
/// are the report's; what it cost is read from the app's record of the chat.
pub(crate) fn reported(held: &Held, session: u32, outcome: Outcome, text: &str) {
    let Some(record) = running_for(held, session) else {
        return;
    };
    let ending = Ending {
        report: Some(dispatchrecord::Report {
            outcome,
            text: text.to_owned(),
            changed: dispatchrecord::Changed::default(),
        }),
        usage: spent(held, session),
    };
    close(held, &record, ending);
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
    let carried_on = me.id.is_some()
        && held.chats().open_now().iter().any(|open| {
            open.session != session
                && held
                    .chats()
                    .chat_at(open.session)
                    .is_some_and(|other| other.id == me.id)
        });
    if carried_on {
        return;
    }
    let Some(record) = dispatchrecord::running_for(held.root(), &me) else {
        return;
    };
    let ending = dispatchrecord::ended_unreported(&record, spent(held, session));
    close(held, &record, ending);
}

/// The app wrote chat `session`'s session record at `path`: the dispatches it asked for are
/// listed on it, and the dispatch it worked on names it.
pub(crate) fn session_recorded(held: &Held, session: u32, path: &str) {
    if let Some(chat) = chat_ref(held, session) {
        dispatchrecord::session_recorded(held.root(), &chat, path);
    }
}

fn running_for(held: &Held, session: u32) -> Option<Record> {
    dispatchrecord::running_for(held.root(), &chat_ref(held, session)?)
}

fn close(held: &Held, record: &Record, ending: Ending) {
    if let Err(why) = dispatchrecord::close(held.root(), &record.id, ending, chrono::Utc::now()) {
        tracing::warn!("purlis: a dispatch's record was not closed ({why})");
    }
}

/// What chat `session`'s harness says its conversation has cost, where it says.
fn spent(held: &Held, session: u32) -> Option<dispatchrecord::Usage> {
    // The conversation the board follows the chat in, else the one the app started it on.
    let conversation = held.board().conversation(session).or_else(|| {
        held.chats()
            .recorded_chat(session)
            .and_then(|chat| chat.resume)
            .map(|id| id.as_str().to_owned())
    })?;
    purlis_core::usage::spent(held.root(), &conversation)
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
    /// `running`, `done`, `blocked`, `failed`, or `handed off` for one that ended owing no
    /// report.
    pub outcome: String,
    /// When it started, as the record keeps it (UTC, RFC 3339).
    pub started: String,
    /// When it ended, the same way; `null` while it runs.
    pub ended: Option<String>,
    /// How long it ran, or has run so far, spelled (`4m 30s`).
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
    /// The persona chat's session, while it is still open: what the row opens.
    pub open_session: Option<u32>,
    /// Else its session record, by its project-relative path, once it wrote one.
    pub session_record: Option<String>,
}

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
        (None, true) => "running",
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
        duration: lasted(record, now),
        needed_you: record.needed_you,
        messages: record.messages,
        cost: usage.and_then(|usage| usage.cost_usd).map(dollars),
        tokens: usage.and_then(|usage| counted(usage.input_tokens, usage.output_tokens)),
        brief: record.brief.clone(),
        report: record.report.as_ref().map(|report| report.text.clone()),
        open_session,
        session_record: record.worker.session_record.clone(),
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
fn counted(input: Option<u64>, output: Option<u64>) -> Option<String> {
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

/// Every dispatch of `held`'s project purlis draws, newest first, and the count of those it
/// will not.
pub(crate) fn rows(held: &Held) -> Dispatches {
    let open: Vec<OpenChat> = held
        .chats()
        .open_now()
        .into_iter()
        .map(|chat| OpenChat {
            session: chat.session,
            id: held.chats().chat_at(chat.session).and_then(|at| at.id),
        })
        .collect();
    let now = chrono::Utc::now();
    let drawn = dispatchrecord::drawn(held.root());
    Dispatches {
        rows: drawn
            .records
            .iter()
            .map(|record| row(record, &open, now))
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

/// One dispatch a chat made, as its session record's tab lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct DispatchMade {
    pub persona: Option<String>,
    pub task: String,
    /// As a row's ([`DispatchRow::outcome`]).
    pub outcome: String,
}

/// The dispatches listed on the session record at `path`, in the order they were made.
pub(crate) fn made_on(root: &Path, path: &str) -> Vec<DispatchMade> {
    let now = chrono::Utc::now();
    dispatchrecord::listed_on(root, path)
        .iter()
        .map(|record| {
            let row = row(record, &[], now);
            DispatchMade {
                persona: row.persona,
                task: row.task,
                outcome: row.outcome,
            }
        })
        .collect()
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
            usage: None,
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
