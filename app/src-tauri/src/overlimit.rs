//! **A task's working time held to its limit, and a session's tokens shown against theirs**
//! (#1512, V100-59): the two dispatch limits that are off until the person sets them
//! ([`purlis_core::dispatchlimits::Limit::off_until_set`]).
//!
//! - **The app's clock** ([`look`]): every [`EVERY`], each open project is looked at. Where
//!   neither limit is set anywhere (the project's files, this machine's, the policy), **nothing
//!   else is read**. Otherwise the dispatch store is listed once for the look.
//! - **Working time** ([`Worked`]): a task's time counts only while it works, its turn running
//!   and no prompt showing: not while it waits on the person, not while it waits on its own
//!   tasks, and not while purlis is not running. The clock adds what it saw since its last look,
//!   at most [`MOST_PER_LOOK`], and keeps the total on the task's record
//!   (`dispatchrecord::worked`), so a task restored after a restart keeps the working minutes it
//!   had.
//! - **Past `minutes-per-task`**, held at the stricter of the dispatch's levels and the
//!   workspace the task works in, the task and everything below it are stopped by the stop Stop
//!   and get its report is ([`crate::stopping::stop_at_a_limit`]): one short turn for a report,
//!   then its end, and the chat that asked is told which limit in purlis's words. **A task the
//!   person is in the middle of is left for the next look**: one showing a prompt or a
//!   question, or one the person has typed into since its harness last spoke, or with such a
//!   task below it.
//! - **Tokens per session are shown, and decide nothing yet** (#1457): the figure is what each
//!   chat's harness reported through a file a chat can write, for its own conversation or
//!   another's. So the clock only notes a session past its limit ([`tokens_shown`]), and its row
//!   says the figure against the limit with "not enforced yet". No dispatch is refused and no
//!   task is stopped by it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use purlis_core::dispatchdecision::Mode;
use purlis_core::dispatchlimits::{self, Limit, Reached};
use purlis_core::dispatchrecord::{self, Record};
use purlis_core::reopen::{Chat, Owed};
use purlis_core::state::State;

use crate::host::ChatBoard;
use crate::planes::{Held, Planes};

/// How often the app looks: a minute is the unit of the time limit.
pub const EVERY: Duration = Duration::from_secs(30);

/// The most working time one look adds: a look that comes late (a machine asleep, a slow look)
/// adds no more than two looks' worth, so time purlis was not watching is not counted.
pub const MOST_PER_LOOK: i64 = 60;

/// A harness's figure, added: tokens in and out, each as reported, a missing half as none.
fn counted(spent: &purlis_core::usage::Spent) -> u64 {
    spent
        .input_tokens
        .unwrap_or(0)
        .saturating_add(spent.output_tokens.unwrap_or(0))
}

/// The chat that asked for `chat` as a task, where it was asked for as one.
fn asker_of(chat: &Chat, number: u32) -> Option<u32> {
    chat.from
        .as_ref()
        .filter(|from| from.mode == Mode::Task && from.chat != number)
        .map(|from| from.chat)
}

/// **The session chat `chat` belongs to**, among `open`: up its task links to the chat no task
/// link leads above. A handoff's chat is a session of its own (V100-69).
pub(crate) fn session_of(open: &[(u32, &Chat)], chat: u32) -> u32 {
    let mut at = chat;
    // A chain is at most a few deep; the bound is for a record that loops.
    for _ in 0..64 {
        let Some(up) = open
            .iter()
            .find(|(number, _)| *number == at)
            .and_then(|(number, one)| asker_of(one, *number))
        else {
            break;
        };
        at = up;
    }
    at
}

/// The open chats of the session `top`: itself and every task below it, at any depth.
fn open_in_session(open: &[(u32, &Chat)], top: u32) -> Vec<u32> {
    let mut found = vec![top];
    let mut at = 0;
    while at < found.len() && found.len() < 4096 {
        let parent = found[at];
        at += 1;
        for (number, chat) in open {
            if asker_of(chat, *number) == Some(parent) && !found.contains(number) {
                found.push(*number);
            }
        }
    }
    found
}

/// **The tokens the session of chat `chat` has used**, among the chats `open`, as their
/// harnesses reported them: its open chats from their conversations' figures, and its ended
/// tasks, at any depth, from what their records kept (`ended`, the store's ended tasks, read
/// once by the caller). Each chat once, by its id. **Shown, and decides nothing.**
pub(crate) fn session_tokens(
    root: &Path,
    board: &dyn ChatBoard,
    open: &[(u32, &Chat)],
    ended: &[Record],
    chat: u32,
) -> u64 {
    let top = session_of(open, chat);
    let mut by_chat: HashMap<String, u64> = HashMap::new();
    let mut known: Vec<String> = Vec::new();
    for number in open_in_session(open, top) {
        let Some((_, one)) = open.iter().find(|(n, _)| *n == number) else {
            continue;
        };
        let key = one
            .identity
            .id
            .clone()
            .unwrap_or_else(|| format!("#{number}"));
        let conversation = board
            .conversation(number)
            .or_else(|| one.resume.as_ref().map(|id| id.as_str().to_owned()));
        let used = conversation
            .and_then(|conversation| purlis_core::usage::spent(root, &conversation))
            .map_or(0, |spent| counted(&spent));
        by_chat.insert(key.clone(), used);
        known.push(key);
    }
    // The ended tasks, found down from the open chats by who asked for each.
    let mut grew = true;
    while grew {
        grew = false;
        for record in ended {
            if record.mode != dispatchrecord::Mode::Task || record.running() {
                continue;
            }
            let (Some(asker), Some(worker)) = (&record.asker.chat.id, &record.worker.chat.id)
            else {
                continue;
            };
            if known.contains(asker) && !by_chat.contains_key(worker) {
                by_chat.insert(worker.clone(), record.usage.as_ref().map_or(0, counted));
                known.push(worker.clone());
                grew = true;
            }
        }
    }
    by_chat
        .values()
        .fold(0_u64, |sum, used| sum.saturating_add(*used))
}

/// One task's working time as the clock keeps it: the seconds it has worked, and the moment of
/// the clock's last look at it.
#[derive(Debug, Clone, Copy)]
struct Clock {
    worked: u64,
    seen: chrono::DateTime<chrono::Utc>,
}

/// **The working time of every task at work**, by project and record, and the sessions past
/// their token limit, by project and chat: what the clock keeps between looks. In memory: a
/// launch reads each task's working time back from its record.
#[derive(Default)]
struct Kept {
    clocks: HashMap<(PathBuf, String), Clock>,
    /// By project and session chat: its tokens and its limit, where it is past it.
    tokens: HashMap<(PathBuf, u32), (u64, u32)>,
}

fn kept() -> std::sync::MutexGuard<'static, Kept> {
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    KEPT.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// What one look found, decided under the lock a dispatch is decided under.
#[derive(Debug, Default)]
struct Found {
    /// Tasks past their time, each with the limit and its record.
    timed: Vec<(u32, Reached, String)>,
    /// Working time to keep on records, by id.
    worked: Vec<(String, u64)>,
    /// Sessions past their token limit: the chat, its tokens and the limit.
    spent: Vec<(u32, u64, u32)>,
}

/// **Whether a task works now**: its turn is running and it shows no prompt or question.
fn works_now(board: &dyn ChatBoard, chat: u32) -> bool {
    let glance = board.glance(chat);
    glance.state == State::Running && !glance.asking
}

/// **Whether the person is in the middle of chat `chat`**: it shows a prompt or a question, or
/// the person has typed into it since its harness last spoke. The clock never stops such a task.
fn the_person_is_in(held: &Held, chat: u32) -> bool {
    held.board().glance(chat).asking || held.tasks().ledger().keyed(chat)
}

/// **Holds every task at work in `held` to its limits, now** (#1512): the clock's one look.
pub(crate) fn look(held: &Arc<Held>) {
    look_at(held, chrono::Utc::now());
}

/// [`look`], as of `now`.
pub(crate) fn look_at(held: &Arc<Held>, now: chrono::DateTime<chrono::Utc>) {
    let root = held.root();
    let files = dispatchlimits::Files::read(root);
    let policy = purlis_core::sandbox::policy::Locks::of(root);
    let timed = files.sets(Limit::MinutesPerTask, policy.dispatch_ceiling());
    let tokens = files.sets(Limit::TokensPerSession, policy.dispatch_ceiling());
    if !tokens {
        kept().tokens.retain(|(plane, _), _| plane != root);
    }
    if !timed && !tokens {
        return;
    }
    let default = purlis_core::start::persona_for_a_new_chat(root);
    let records = dispatchrecord::list(root);
    let found = held.chats().deciding_over(|open, _| {
        let mut found = Found::default();
        if timed {
            time_each(held, root, open, &records, &files, &policy, now, &mut found);
        }
        if tokens {
            let ended: Vec<Record> = records
                .iter()
                .filter(|record| !record.running())
                .cloned()
                .collect();
            for (top, chat) in open.iter().filter(|(n, chat)| asker_of(chat, *n).is_none()) {
                let pair = purlis_core::dispatchdecision::pair_of(chat, None, default.as_deref());
                let workspace = chat
                    .cwd
                    .as_deref()
                    .and_then(|cwd| purlis_core::active::workspace_of_tree(root, cwd));
                let limits = files.in_force(
                    workspace.as_deref(),
                    pair.asking.as_deref(),
                    None,
                    policy.dispatch_ceiling(),
                );
                if limits.tokens_per_session.is_none() {
                    continue;
                }
                let used = session_tokens(root, held.board(), open, &ended, *top);
                if let Some(limit) = dispatchlimits::tokens_past(&limits, used) {
                    found.spent.push((*top, used, limit));
                }
            }
        }
        found
    });
    for (id, worked) in found.worked {
        if let Err(why) = dispatchrecord::worked(root, &id, worked) {
            tracing::warn!("purlis: a task's working time was not kept on its record ({why})");
        }
    }
    if tokens {
        let mut kept = kept();
        kept.tokens.retain(|(plane, _), _| plane != root);
        for (top, used, limit) in &found.spent {
            kept.tokens.insert((root.to_owned(), *top), (*used, *limit));
        }
    }
    for (task, reached, id) in found.timed {
        // **Never under the person's hands**: left for the next look.
        let below = crate::handoff::at_work_below(held, task);
        if std::iter::once(task)
            .chain(below)
            .any(|chat| the_person_is_in(held, chat))
        {
            continue;
        }
        crate::stopping::stop_at_a_limit(held, task, reached);
        kept().clocks.remove(&(root.to_owned(), id));
    }
}

/// The working time of each task at work in `open`, and those past `minutes-per-task`.
#[allow(clippy::too_many_arguments)]
fn time_each(
    held: &Held,
    root: &Path,
    open: &[(u32, &Chat)],
    records: &[Record],
    files: &dispatchlimits::Files,
    policy: &purlis_core::sandbox::policy::Locks,
    now: chrono::DateTime<chrono::Utc>,
    found: &mut Found,
) {
    let mut kept = kept();
    // A task that has ended keeps no clock here.
    kept.clocks.retain(|(plane, id), _| {
        plane != root
            || records
                .iter()
                .any(|record| record.running() && &record.id == id)
    });
    let tasks = open.iter().filter(|(number, chat)| {
        asker_of(chat, *number).is_some()
            && chat
                .from
                .as_ref()
                .is_some_and(|from| from.report == Owed::Due)
    });
    for (task, chat) in tasks {
        let Some(record) = records.iter().find(|record| {
            record.running()
                && dispatchrecord::named(
                    &record.worker.chat,
                    chat.identity.id.as_deref(),
                    Some(*task),
                )
        }) else {
            continue;
        };
        let key = (root.to_owned(), record.id.clone());
        let clock = kept.clocks.entry(key).or_insert(Clock {
            // A launch reads the working time back from the record: what it had, and no more.
            worked: record.worked,
            seen: now,
        });
        if works_now(held.board(), *task) {
            let since = (now - clock.seen).num_seconds().clamp(0, MOST_PER_LOOK);
            let before = clock.worked;
            clock.worked = clock
                .worked
                .saturating_add(u64::try_from(since).unwrap_or(0));
            if clock.worked / 60 != before / 60 {
                found.worked.push((record.id.clone(), clock.worked));
            }
        }
        clock.seen = now;
        // The dispatch's levels, and the workspace it works in where that is another: the
        // stricter of the two, as a dispatch into another workspace is held to both.
        let asked = files.in_force(
            record.asker.workspace.as_deref(),
            record.asker.chat.persona.as_deref(),
            record.persona.as_deref(),
            policy.dispatch_ceiling(),
        );
        let there = record
            .place
            .workspace
            .as_deref()
            .filter(|there| Some(*there) != record.asker.workspace.as_deref())
            .map(|there| {
                files.in_force(
                    Some(there),
                    record.asker.chat.persona.as_deref(),
                    record.persona.as_deref(),
                    policy.dispatch_ceiling(),
                )
            });
        let mut limits = asked;
        limits.minutes_per_task = dispatchlimits::stricter(
            limits.minutes_per_task,
            there.and_then(|there| there.minutes_per_task),
        );
        if let Some(reached) = dispatchlimits::time_reached(&limits, clock.worked) {
            found.timed.push((*task, reached, record.id.clone()));
        }
    }
}

/// **What session chat `chat`'s row says of its tokens**, where the clock last found it past
/// its token limit: the figure against the limit, and that it is not enforced yet. Read from
/// what the clock kept: no file is read for a row.
pub(crate) fn tokens_shown(held: &Held, chat: u32) -> Option<crate::atlimit::AtLimit> {
    let (used, limit) = *kept().tokens.get(&(held.root().to_owned(), chat))?;
    let used_said = dispatchlimits::spelled(used);
    let limit_said = dispatchlimits::spelled(u64::from(limit));
    Some(crate::atlimit::AtLimit {
        limit,
        row: format!("past its token limit ({used_said} of {limit_said}) · not enforced yet"),
        said: format!(
            "This session has used {used_said} tokens, its own chat and its tasks together as \
             their harnesses reported them, and its limit is {limit_said}. The token limit is \
             not enforced yet: a chat can alter the figure it counts, so nothing is refused or \
             stopped by it. You can change it in Settings › Project › Dispatch."
        ),
    })
}

/// **Starts the app's clock**: every [`EVERY`], each open project is looked at ([`look`]). On a
/// thread of its own for the life of the app; a look that panics is said and the clock goes on.
pub(crate) fn keep_looking(app: tauri::AppHandle) {
    use tauri::Manager;
    let spawned = std::thread::Builder::new()
        .name("purlis-limits".into())
        .spawn(move || {
            loop {
                std::thread::sleep(EVERY);
                let Some(planes) = app.try_state::<Planes>() else {
                    continue;
                };
                for id in planes.open_now() {
                    let Ok(held) = planes.held(&id) else {
                        continue;
                    };
                    let looked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        look(&held);
                    }));
                    if looked.is_err() {
                        tracing::warn!(
                            "purlis: the clock that holds tasks to their time limit failed one \
                             look at a project, and looks again in {} seconds",
                            EVERY.as_secs()
                        );
                    }
                }
            }
        });
    if let Err(why) = spawned {
        tracing::warn!(
            "purlis: the clock that holds tasks to their time limit could not be started \
             ({why}), so no task is stopped at its time limit"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_harness_s_figure_adds_its_halves_and_a_missing_half_as_none() {
        let both = purlis_core::usage::Spent {
            input_tokens: Some(1_000),
            output_tokens: Some(250),
            cost_usd: None,
        };
        assert_eq!(counted(&both), 1_250);
        let half = purlis_core::usage::Spent {
            input_tokens: None,
            output_tokens: Some(250),
            cost_usd: Some(0.5),
        };
        assert_eq!(counted(&half), 250);
        assert_eq!(counted(&purlis_core::usage::Spent::default()), 0);
    }
}
