//! **One answer for the tasks of one session that hit the same sandbox block** (#1508, V100-57).
//!
//! Several tasks of one session that are blocked on the same host, or on writing the same
//! folder, are asked about in one Notice on the session's tab, and the one answer applies to
//! each task the Notice listed. This is the core's half: it holds the block each open chat is
//! on now ([`Blocks`]), and judges and keeps an answer only against that.
//!
//! # What holds (D-1508-9, fail closed)
//!
//! - **An answer names the exact blocks the person saw**: each task with the block shown for
//!   it ([`SeenBlock`]). The answer is refused whole if any task is not held on that block now
//!   (it was never blocked, it was answered already, it moved on to another block, or it
//!   ended), if the blocks are not one block, or if any task is not recorded below the
//!   session. So a press can grant only a task whose block was on screen, as the dispatch
//!   question's answer is bound to what it showed. Keep blocked is checked the same way.
//! - **Who asked whom is purlis's own record**, the lineage the app wrote when it started each
//!   task ([`purlis_core::reopen::HandedFrom`]). The session itself is never one of its tasks: a
//!   permission given to a session does not reach its tasks, and an answer about its tasks does
//!   not reach the session.
//! - **"For this chat" is each task's own grant**, held under that task's id and judged against
//!   that task's own folder. Every task is judged before anything is kept. If keeping fails part
//!   way (the audit cannot be written), the answer says which tasks it allowed, and only those.
//! - **"For me" and "for the project" are kept once**, audited once more for each other task
//!   listed, and every task listed is owed the restart that takes it.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use purlis_core::sandbox::grant::{self, Level, What};

use crate::planes::{PlaneId, Planes};
use crate::sandboxing::{GrantLevel, GrantWhat};

/// How far up a lineage is followed before it is taken to reach nothing: far past any depth a
/// dispatch is allowed, so only a record that loops stops here.
const MOST_DEPTH: usize = 64;

/// The most blocks the app holds for one chat at once, as the window keeps them
/// (`sandboxBlocks.ts`): one per operation and kind, the oldest going first. The hosts of one
/// block count once ([`MOST_HOSTS_PER_BLOCK`]).
const MOST_HELD_PER_CHAT: usize = 5;

/// The most hosts one block of a chat names at once, as the window's one Notice lists them
/// (#1637): the oldest going first. As many as one tool result can be refused.
const MOST_HOSTS_PER_BLOCK: usize = purlis_core::sandboxblock::AT_MOST_PER_RESULT;

/// **The host or folder a block names, as a grant matches it**: a host lowered, without a
/// trailing dot, and without its scheme's default port (`:443`, `:80`), which the grant for the
/// bare host covers; a folder as the core offered it. None for what no answer names.
pub fn normalised(what: GrantWhat, target: &str) -> Option<String> {
    match what {
        GrantWhat::Host => {
            let host = grant::host(target).ok()?;
            let said = host.to_string();
            Some(match host.port() {
                Some(port @ (443 | 80)) => said
                    .strip_suffix(&format!(":{port}"))
                    .unwrap_or(&said)
                    .to_owned(),
                _ => said,
            })
        }
        GrantWhat::Write => (!target.is_empty()).then(|| target.to_owned()),
        // Neither is ever a block's: each is granted from its own Notice (#1430, #1362).
        GrantWhat::Vault | GrantWhat::PersonaHosts => None,
    }
}

/// One block a chat is held on: its operation and kind, by their words, and what it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldBlock {
    pub operation: String,
    pub kind: String,
    pub what: GrantWhat,
    /// [`normalised`]; empty for a host the block did not name, which the person types on the
    /// chat's own Notice ([`HeldBlock::unnamed_host`]).
    pub target: String,
}

impl HeldBlock {
    /// A block on a host its report did not name: only the chat's own Notice answers it, with
    /// the host the person types (#1538). No question for several tasks matches it.
    pub fn unnamed_host(operation: &str, kind: &str) -> Self {
        Self {
            operation: operation.to_owned(),
            kind: kind.to_owned(),
            what: GrantWhat::Host,
            target: String::new(),
        }
    }

    /// A host its report named: one of several a chat may be held on at once under one
    /// operation and kind (#1637).
    fn names_a_host(&self) -> bool {
        self.what == GrantWhat::Host && !self.target.is_empty()
    }

    /// Whether `other` is asked about in the same Notice: the same operation and kind, and both
    /// naming a host or neither. Hosts named alike are listed together in one Notice; any other
    /// block is one per operation and kind, the newest replacing the one before.
    fn shares_a_notice(&self, other: &Self) -> bool {
        self.operation == other.operation
            && self.kind == other.kind
            && self.names_a_host() == other.names_a_host()
    }
}

/// **The block each open chat is held on now**, as the app heard it: one per operation and
/// kind, the newest replacing the one before as the window's Notice does, except that each host
/// a report named is held beside the others its one Notice lists (#1637). Cleared when an
/// answer takes it, and when the chat ends ([`crate::chats::Chats::close`]). In memory only.
#[derive(Default)]
pub struct Blocks {
    held: Mutex<HashMap<u32, Vec<HeldBlock>>>,
    /// The turn each open chat last had a block Notice raised in, by the count of turns its
    /// board had then (#1663): what the chat that asked for it is told it waits on.
    noticed: Mutex<HashMap<u32, u32>>,
}

impl Blocks {
    fn held(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Vec<HeldBlock>>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Chat `session` is held on `block` now: beside the other hosts its Notice lists, where it
    /// names a host (#1637), and otherwise in place of the block of its operation and kind.
    pub fn heard(&self, session: u32, block: HeldBlock) {
        let mut held = self.held();
        let mine = held.entry(session).or_default();
        if block.names_a_host() {
            mine.retain(|one| *one != block);
        } else {
            mine.retain(|one| !one.shares_a_notice(&block));
        }
        mine.push(block.clone());
        let hosts = mine
            .iter()
            .filter(|one| one.shares_a_notice(&block))
            .count();
        if hosts > MOST_HOSTS_PER_BLOCK
            && let Some(oldest) = mine.iter().position(|one| one.shares_a_notice(&block))
        {
            mine.remove(oldest);
        }
        // The oldest Notice goes first, with every host it lists.
        loop {
            let mut notices: Vec<&HeldBlock> = Vec::new();
            for one in mine.iter() {
                if !notices.iter().any(|seen| seen.shares_a_notice(one)) {
                    notices.push(one);
                }
            }
            if notices.len() <= MOST_HELD_PER_CHAT {
                break;
            }
            let oldest = notices[0].clone();
            mine.retain(|one| !one.shares_a_notice(&oldest));
        }
    }

    /// Whether chat `session` is held on `block` now.
    pub fn holds(&self, session: u32, block: &HeldBlock) -> bool {
        self.held()
            .get(&session)
            .is_some_and(|mine| mine.contains(block))
    }

    /// `block` of chat `session` was answered: it is held no longer.
    pub fn answered(&self, session: u32, block: &HeldBlock) {
        self.noticed().remove(&session);
        let mut held = self.held();
        if let Some(mine) = held.get_mut(&session) {
            mine.retain(|one| one != block);
            if mine.is_empty() {
                held.remove(&session);
            }
        }
    }

    /// Chat `session` ended: nothing is held for it.
    pub fn ended(&self, session: u32) {
        self.held().remove(&session);
        self.noticed().remove(&session);
    }

    fn noticed(&self) -> std::sync::MutexGuard<'_, HashMap<u32, u32>> {
        self.noticed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A Notice went up for a block of chat `session`, in the turn its board counts as `turn`
    /// (#1663), whatever the Notice offers.
    pub fn raised(&self, session: u32, turn: u32) {
        self.noticed().insert(session, turn);
    }

    /// **Whether chat `session` waits on the person over a block's Notice** (#1663): a Notice
    /// went up in the turn its board counts as `turn` now, and that turn has ended
    /// (`waiting`). A new turn, a start again (whose count begins anew), an answer and an end
    /// each move it on.
    pub fn waits_on_a_notice(&self, session: u32, turn: u32, waiting: bool) -> bool {
        waiting && self.noticed().get(&session) == Some(&turn)
    }
}

/// Holds the block the window is about to be told of, where an answer could name it: a host
/// or a folder to write, named whole, or a host the report did not name, which the person
/// types on the chat's own Notice.
pub fn heard(chats: &crate::chats::Chats, told: &crate::hooks::ChatBlocked) {
    let what = match told.offer {
        crate::hooks::BlockOffer::Host => GrantWhat::Host,
        crate::hooks::BlockOffer::Write => GrantWhat::Write,
        _ => return,
    };
    let block = match told.target.as_deref() {
        None if what == GrantWhat::Host => HeldBlock::unnamed_host(&told.operation, &told.kind),
        None => return,
        Some(target) => {
            let Some(target) = normalised(what, target) else {
                return;
            };
            HeldBlock {
                operation: told.operation.clone(),
                kind: told.kind.clone(),
                what,
                target,
            }
        }
    };
    chats.blocks().heard(told.session, block);
}

/// **What one chat's own block Notice answers** (#1538): the block's operation and kind, by the
/// words the window was told, what it offers, and the host or folder Allow names: the one the
/// Notice showed whole, or the host the person typed where the block named none.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct BlockShown {
    pub operation: String,
    pub kind: String,
    pub what: GrantWhat,
    pub target: String,
}

/// **The block one chat's own Notice answers** (#1538, as D-1508-9 binds the question for
/// several tasks): chat `session` held now on the block of that operation and kind, offering
/// `what`, and naming `target` as a grant matches it, or naming no host where the person typed
/// `target`. Anything else (a block never heard, answered already, replaced by a newer one, or
/// of a chat that ended) refuses the Allow whole, before anything is judged or kept.
pub fn shown_one(blocks: &Blocks, session: u32, shown: &BlockShown) -> Result<HeldBlock, String> {
    let BlockShown {
        operation,
        kind,
        what,
        target,
    } = shown;
    let (what, target) = (*what, target.as_str());
    let named = normalised(what, target).map(|target| HeldBlock {
        operation: operation.to_owned(),
        kind: kind.to_owned(),
        what,
        target,
    });
    let typed = (what == GrantWhat::Host).then(|| HeldBlock::unnamed_host(operation, kind));
    named
        .into_iter()
        .chain(typed)
        .find(|block| blocks.holds(session, block))
        .ok_or_else(|| {
            format!(
                "Nothing was allowed: chat {session} is not blocked on what this Notice showed any \
                 more. It was answered already, or the chat moved on or ended."
            )
        })
}

/// The chat that asked for chat `session` as a task, by the app's own record of it; none for a
/// chat no task link records (the person started it, or a handoff did).
pub type AskerOf<'a> = &'a dyn Fn(u32) -> Option<u32>;

/// **Whether `task` is a task below `session`**, at any depth, by `asker_of`: the app's record
/// of who asked whom. A chat is never below itself.
pub fn below(asker_of: AskerOf<'_>, session: u32, task: u32) -> bool {
    let mut at = task;
    for _ in 0..MOST_DEPTH {
        match asker_of(at) {
            Some(asker) if asker == session => return true,
            Some(asker) if asker != at => at = asker,
            _ => return false,
        }
    }
    false
}

/// **One task's block, as the person saw it** on the question: what the window sends back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct SeenBlock {
    /// The task's number.
    pub task: u32,
    /// The block's operation and kind, by the words the window was told.
    pub operation: String,
    pub kind: String,
    pub what: GrantWhat,
    /// The host or folder shown for this task.
    pub target: String,
}

/// What one answer to several tasks did (#1508).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct TasksAnswered {
    /// The sentence the Notice says.
    pub said: String,
    /// The tasks it answered: each listed one, or, where keeping failed part way, those it
    /// allowed before it failed. The window puts their blocks away and restarts them.
    pub answered: Vec<u32>,
}

/// What [`checked`] and [`answered`] read of the app, so they read no chat store of their own.
pub struct Reading<'a> {
    /// Who asked whom ([`below`]).
    pub asker_of: AskerOf<'a>,
    /// Whether a task is held on a block now ([`Blocks::holds`]).
    pub holds: &'a dyn Fn(u32, &HeldBlock) -> bool,
}

/// **What an answer to several tasks of `session` is checked against before anything is done**
/// (D-1508-9): one block, named the same for every task; each task held on it now; each
/// recorded below `session`, none of them `session`, none twice. Answers the tasks and the one
/// block, or why the whole answer is refused.
pub fn checked(
    session: u32,
    seen: &[SeenBlock],
    reading: &Reading<'_>,
) -> Result<(Vec<u32>, HeldBlock), String> {
    let refused = |why: &str| Err(format!("Nothing was answered: {why}"));
    let [first, ..] = seen else {
        return refused("no task was named.");
    };
    let held_of = |one: &SeenBlock| {
        normalised(one.what, &one.target).map(|target| HeldBlock {
            operation: one.operation.clone(),
            kind: one.kind.clone(),
            what: one.what,
            target,
        })
    };
    let Some(block) = held_of(first) else {
        return refused("the question names nothing that can be answered.");
    };
    let mut tasks = Vec::with_capacity(seen.len());
    for one in seen {
        if held_of(one).as_ref() != Some(&block) {
            return refused(
                "the tasks listed are not on one block, so one answer cannot cover them.",
            );
        }
        if one.task == session {
            return refused(
                "a session is not one of its own tasks. Answer the session's own block on it.",
            );
        }
        if tasks.contains(&one.task) {
            return Err(format!(
                "Nothing was answered: chat {} was named twice.",
                one.task
            ));
        }
        if !below(reading.asker_of, session, one.task) {
            return Err(format!(
                "Nothing was answered: purlis has no record of chat {} as a task of this \
                 session, and one answer covers only the session's own tasks.",
                one.task
            ));
        }
        if !(reading.holds)(one.task, &block) {
            return Err(format!(
                "Nothing was answered: chat {} is not blocked on what the question showed any \
                 more. The question is shown again as it is now.",
                one.task
            ));
        }
        tasks.push(one.task);
    }
    Ok((tasks, block))
}

/// Judges what allowing a block for one task would grant.
pub type Judge<'a> = &'a dyn Fn(u32, &HeldBlock) -> Result<(What, Level), String>;

/// What [`answered`] is told to do with the app.
pub struct Doing<'a> {
    /// What allowing `block` for one task would grant, judged by the core
    /// ([`crate::sandboxing::judged`]).
    pub judge: Judge<'a>,
    /// Keeps one grant for one task: audited, kept and the task owed its restart
    /// ([`crate::sandboxing::kept`]).
    pub keep: &'a mut dyn FnMut(u32, &What, Level) -> Result<(), String>,
    /// Audits a grant kept once for everyone as reaching one more task.
    pub note: &'a dyn Fn(u32, &What, Level) -> Result<(), String>,
    /// Owes one task a restart that tells it what it was allowed.
    pub owe: &'a dyn Fn(u32, String),
}

/// **The one Allow to the tasks a Notice listed** ([module](self)): [`checked`], every task
/// judged, then kept for each task (this chat) or once (for me, for the project) with each task
/// audited and owed its restart. Nothing is kept unless every check passed.
pub fn answered(
    session: u32,
    seen: &[SeenBlock],
    level: GrantLevel,
    reading: &Reading<'_>,
    doing: Doing<'_>,
) -> Result<TasksAnswered, String> {
    let (tasks, block) = checked(session, seen, reading)?;
    let mut judged = Vec::with_capacity(tasks.len());
    for task in &tasks {
        let (what, level) = (doing.judge)(*task, &block)
            .map_err(|why| format!("Nothing was allowed: for chat {task}, {why}"))?;
        judged.push((*task, what, level));
    }
    let count = tasks.len();
    if Level::from(level) == Level::Chat {
        let mut allowed = Vec::new();
        for (task, what, level) in &judged {
            if let Err(why) = (doing.keep)(*task, what, *level) {
                if allowed.is_empty() {
                    return Err(format!("Nothing was allowed: {why}"));
                }
                return Ok(TasksAnswered {
                    said: format!(
                        "It was allowed for chat {} only, and not for the rest: {why}",
                        numbers(&allowed)
                    ),
                    answered: allowed,
                });
            }
            allowed.push(*task);
        }
        return Ok(TasksAnswered {
            said: if count == 1 {
                "Allowed for this task alone, not for the chat that asked it. It restarts on \
                 the same conversation once its turn ends, and is told to retry."
                    .to_owned()
            } else {
                format!(
                    "Allowed for each of these {count} tasks on its own, not for the chat that \
                     asked them. Each restarts on the same conversation once its turn ends, and \
                     is told to retry."
                )
            },
            answered: allowed,
        });
    }
    // Kept once: every task was judged to the same grant, or it is not one answer.
    let [(first, what, level), rest @ ..] = judged.as_slice() else {
        return Err("Nothing was allowed: no task was named.".to_owned());
    };
    if rest.iter().any(|(_, other, _)| other != what) {
        return Err(
            "Nothing was allowed: these tasks do not want the same thing, so one answer cannot \
             cover them. Answer each on its own."
                .to_owned(),
        );
    }
    (doing.keep)(*first, what, *level).map_err(|why| format!("Nothing was allowed: {why}"))?;
    let told = grant::told(what, *level);
    for (task, _, _) in rest {
        if let Err(why) = (doing.note)(*task, what, *level) {
            // The grant stands, audited once; only this task's line of it is missing.
            tracing::warn!("purlis: a sandbox grant reaching chat {task} was not audited ({why})");
        }
        (doing.owe)(*task, told.clone());
    }
    Ok(TasksAnswered {
        said: format!(
            "Allowed {}. The {count} {} restart on the same conversation once each one's turn \
             ends, and are told to retry.",
            level.said(),
            if count == 1 { "task" } else { "tasks" }
        ),
        answered: tasks,
    })
}

/// `1, 4 and 5`.
fn numbers(of: &[u32]) -> String {
    let said: Vec<String> = of.iter().map(u32::to_string).collect();
    match said.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The chat that asked for chat `session` as a task, as `chats` records it while it is open.
fn asker_in(chats: &crate::chats::Chats, session: u32) -> Option<u32> {
    let from = chats.recorded_chat(session)?.from?;
    (from.mode == purlis_core::reopen::Mode::Task).then_some(from.chat)
}

/// Runs `with` on what `chats` is read for: its lineage and the blocks it holds.
fn reading<R>(chats: &crate::chats::Chats, with: impl FnOnce(&Reading<'_>) -> R) -> R {
    with(&Reading {
        asker_of: &|chat| asker_in(chats, chat),
        holds: &|task, block| chats.blocks().holds(task, block),
    })
}

/// [`allow_sandbox_block_for_tasks`] for the project at `root` on `machine`, with `chats` and
/// `audit` the app's: [`answered`] with the real judging and keeping, then the blocks of the
/// tasks it answered let go.
pub fn allow_for_tasks(
    root: &std::path::Path,
    machine: &purlis_core::sandbox::Machine,
    chats: &crate::chats::Chats,
    (session, seen, level): (u32, &[SeenBlock], GrantLevel),
    audit: crate::sandboxing::Audit<'_>,
    at: u64,
) -> Result<TasksAnswered, String> {
    let done = reading(chats, |reading| {
        answered(
            session,
            seen,
            level,
            reading,
            Doing {
                judge: &|task, block| {
                    crate::sandboxing::judged(
                        root,
                        machine,
                        chats.folder_of(task).as_deref(),
                        task,
                        (block.what, &block.target, level),
                    )
                },
                keep: &mut |task, what, level| {
                    crate::sandboxing::kept(root, chats, task, (what, level), audit, at)
                },
                note: &|task, what, level| {
                    audit(
                        Some(task),
                        &grant::Audited {
                            granted: true,
                            what: what.word(),
                            target: &what.target(),
                            level,
                        },
                    )
                },
                owe: &|task, told| chats.owe_restart(task, told),
            },
        )
    })?;
    let_go(chats, seen, &done.answered);
    Ok(done)
}

/// The blocks of `answered` among `seen` are answered: the app holds them no longer.
fn let_go(chats: &crate::chats::Chats, seen: &[SeenBlock], answered: &[u32]) {
    for one in seen.iter().filter(|one| answered.contains(&one.task)) {
        if let Some(target) = normalised(one.what, &one.target) {
            chats.blocks().answered(
                one.task,
                &HeldBlock {
                    operation: one.operation.clone(),
                    kind: one.kind.clone(),
                    what: one.what,
                    target,
                },
            );
        }
    }
}

/// **Allow on the one question for several tasks of chat `session`** (#1508, V100-57): `seen`
/// is each task with the block the question showed for it. The whole answer is refused unless
/// every task is held on that one block now and is a task below `session` by the app's own
/// record. Each is judged on its own, and the answer is kept for each task alone (`chat`), or
/// once for every chat of the project on this machine (`you`) or for everyone in it
/// (`project`), with each task owed a restart. The window then restarts each once its turn has
/// ended (`restart_chat`).
#[tauri::command]
#[specta::specta]
pub fn allow_sandbox_block_for_tasks(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    seen: Vec<SeenBlock>,
    level: GrantLevel,
) -> Result<TasksAnswered, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    allow_for_tasks(
        &root,
        &purlis_core::sandbox::Machine::this(),
        held.chats(),
        (session, &seen, level),
        &crate::network::recorded(
            &|number, audited| held.hooks().record_grant(&root, number, audited),
            planes.network(),
            &root,
            held.chats(),
        ),
        crate::sandboxing::now_secs(),
    )
}

/// **Keep blocked on the one question for several tasks of chat `session`** (#1508): checked
/// as an Allow is ([`checked`]), then each task's block is let go. Nothing is granted.
#[tauri::command]
#[specta::specta]
pub fn keep_sandbox_block_for_tasks(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    seen: Vec<SeenBlock>,
) -> Result<Vec<u32>, String> {
    let held = planes.held(&plane)?;
    keep_blocked(held.chats(), session, &seen)
}

/// [`keep_sandbox_block_for_tasks`] on `chats`.
pub fn keep_blocked(
    chats: &crate::chats::Chats,
    session: u32,
    seen: &[SeenBlock],
) -> Result<Vec<u32>, String> {
    let (tasks, _) = reading(chats, |reading| checked(session, seen, reading))?;
    let_go(chats, seen, &tasks);
    Ok(tasks)
}

impl Blocks {
    /// **Every block each open chat is held on now**, by session, lowest first: what the asks
    /// registry lists as sandbox host asks (#1690).
    pub fn every(&self) -> Vec<(u32, HeldBlock)> {
        let held = self.held();
        let mut every: Vec<(u32, HeldBlock)> = held
            .iter()
            .flat_map(|(session, blocks)| blocks.iter().map(|block| (*session, block.clone())))
            .collect();
        every.sort_by_key(|(session, _)| *session);
        every
    }
}

#[cfg(test)]
#[path = "taskblocks_tests.rs"]
mod tests;
