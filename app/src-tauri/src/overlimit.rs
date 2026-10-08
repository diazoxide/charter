//! **A session's tokens and a task's time, held to their limits** (#1512, V100-59): the two
//! dispatch limits that are off until the person sets them
//! ([`purlis_core::dispatchlimits::Limit::off_until_set`]).
//!
//! - **What a session has used** ([`session_tokens`]): the chat the person started and every
//!   task below it, open or ended, each chat once, as their harnesses reported them. An open
//!   chat is read from its conversation's figure (`purlis_core::usage::spent`, the source the
//!   Dispatches tab and the tab menu's total read); an ended task from what its record kept
//!   when it ended (`dispatchrecord::Record::usage`), so a finished task still counts. A chat
//!   whose harness reported nothing adds nothing: never a guess, and so the limit cannot bind
//!   on a harness that reports no tokens (Settings says so beside the limit).
//! - **The app's clock** ([`look`]): every [`EVERY`], each open project's tasks at work are
//!   held to the limits as the files say them now. A task past `minutes-per-task` is stopped
//!   with everything below it; every task at work below a session past `tokens-per-session`
//!   is stopped, and the session's row says it is at its token limit. Both by the stop Stop
//!   and get its report is ([`crate::stopping::stop_at_a_limit`]): one short turn for a
//!   report, then its end, and the chat that asked is told which limit in purlis's words.
//!   Nothing is read where no limit is set.
//!
//! **A reported figure** (D-1452-12): the tokens are what a chat's harness relayed through a
//! file a chat can write. The limit is a brake on work nobody is watching, read from the best
//! figure there is, and not a boundary a chat cannot cross.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use purlis_core::dispatchdecision::Mode;
use purlis_core::dispatchlimits::{self, Reached};
use purlis_core::dispatchrecord;
use purlis_core::reopen::{Chat, Owed};

use crate::host::ChatBoard;
use crate::planes::{Held, Planes};

/// How often the app holds the tasks at work to their limits: a minute is the unit of the
/// time limit, and a session's tokens move with its turns.
pub const EVERY: Duration = Duration::from_secs(30);

/// A harness's figure, added: tokens in and out, each as reported, a missing half as none.
fn counted(spent: &purlis_core::usage::Spent) -> u64 {
    spent
        .input_tokens
        .unwrap_or(0)
        .saturating_add(spent.output_tokens.unwrap_or(0))
}

/// The chat that asked for `from`'s chat as a task, where it was asked for as one.
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
/// tasks, at any depth, from what their records kept. Each chat once, by its id.
pub(crate) fn session_tokens(
    root: &Path,
    board: &dyn ChatBoard,
    open: &[(u32, &Chat)],
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
    let ended: Vec<dispatchrecord::Record> = dispatchrecord::list(root)
        .into_iter()
        .filter(|record| record.mode == dispatchrecord::Mode::Task && !record.running())
        .collect();
    let mut grew = true;
    while grew {
        grew = false;
        for record in &ended {
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

/// What the clock found to stop, decided under the lock a dispatch is decided under.
#[derive(Debug, Default)]
struct Found {
    /// Tasks past their time, each with the limit.
    timed: Vec<(u32, Reached)>,
    /// Sessions past their tokens, each with the limit.
    spent: Vec<(u32, Reached)>,
}

/// **Holds every task at work in `held` to its limits, now** (#1512): the clock's one look.
pub(crate) fn look(held: &Arc<Held>) {
    look_at(held, chrono::Utc::now());
}

/// [`look`], as of `now`.
pub(crate) fn look_at(held: &Arc<Held>, now: chrono::DateTime<chrono::Utc>) {
    let root = held.root();
    let default = purlis_core::start::persona_for_a_new_chat(root);
    let found = held.chats().deciding_over(|open, _| {
        let mut found = Found::default();
        let at_work: Vec<u32> = open
            .iter()
            .filter(|(number, chat)| {
                asker_of(chat, *number).is_some()
                    && chat
                        .from
                        .as_ref()
                        .is_some_and(|from| from.report == Owed::Due)
            })
            .map(|(number, _)| *number)
            .collect();
        if at_work.is_empty() {
            return found;
        }
        let running: Vec<dispatchrecord::Record> = dispatchrecord::list(root)
            .into_iter()
            .filter(dispatchrecord::Record::running)
            .collect();
        for task in &at_work {
            let Some((_, chat)) = open.iter().find(|(n, _)| n == task) else {
                continue;
            };
            let Some(record) = running.iter().find(|record| {
                dispatchrecord::named(
                    &record.worker.chat,
                    chat.identity.id.as_deref(),
                    Some(*task),
                )
            }) else {
                continue;
            };
            let limits = dispatchlimits::of(
                root,
                record.asker.workspace.as_deref(),
                record.asker.chat.persona.as_deref(),
                record.persona.as_deref(),
            );
            let Some(started) = chrono::DateTime::parse_from_rfc3339(&record.started).ok() else {
                continue;
            };
            let worked = (now - started.with_timezone(&chrono::Utc)).num_seconds();
            if let Some(reached) = dispatchlimits::time_reached(&limits, worked) {
                found.timed.push((*task, reached));
            }
        }
        let mut tops: Vec<u32> = at_work.iter().map(|task| session_of(open, *task)).collect();
        tops.sort_unstable();
        tops.dedup();
        for top in tops {
            let Some((_, chat)) = open.iter().find(|(n, _)| *n == top) else {
                continue;
            };
            let pair = purlis_core::dispatchdecision::pair_of(chat, None, default.as_deref());
            let workspace = chat
                .cwd
                .as_deref()
                .and_then(|cwd| purlis_core::active::workspace_of_tree(root, cwd));
            let limits =
                dispatchlimits::of(root, workspace.as_deref(), pair.asking.as_deref(), None);
            if limits.tokens_per_session.is_none() {
                continue;
            }
            let used = session_tokens(root, held.board(), open, top);
            if let Some(reached) = dispatchlimits::tokens_reached_at_work(&limits, used) {
                found.spent.push((top, reached));
            }
        }
        found
    });
    for (task, reached) in found.timed {
        crate::stopping::stop_at_a_limit(held, &[task], reached, None);
    }
    for (top, reached) in found.spent {
        let tasks = crate::handoff::at_work_below(held, top);
        let stopped = crate::stopping::stop_at_a_limit(held, &tasks, reached, Some(top));
        if stopped > 0 {
            held.at_limits().at_its_tokens(top);
            held.rows_changed();
        }
    }
}

/// **Starts the app's clock**: every [`EVERY`], each open project is looked at ([`look`]). On a
/// thread of its own for the life of the app.
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
                    if let Ok(held) = planes.held(&id) {
                        look(&held);
                    }
                }
            }
        });
    if let Err(why) = spawned {
        tracing::warn!(
            "purlis: the clock that holds tasks to their time and token limits could not be \
             started ({why}), so those two limits refuse new tasks and stop none at work"
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
