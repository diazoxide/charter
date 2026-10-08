//! **One answer for the tasks of one session that hit the same sandbox block** (#1508, V100-57).
//!
//! Several tasks of one session that are blocked on the same host, or on writing the same
//! folder, are asked about in one Notice on the session's tab, and the one answer applies to
//! each task the Notice listed. This is the core's half: the window sends the session, the
//! tasks it showed and what it showed them wanting, and this judges and keeps it.
//!
//! # What holds
//!
//! - **Who asked whom is purlis's own record**, the lineage the app wrote when it started each
//!   task ([`purlis_core::reopen::HandedFrom`]), never anything a chat says about itself. A
//!   listed chat that the record does not put below the session gets nothing, and neither does
//!   any other listed chat: the answer is refused whole.
//! - **The answer covers the tasks listed, and no other chat.** The session itself is never
//!   one of them: a permission given to a session does not reach its tasks, and an answer about
//!   its tasks does not reach the session. A task that hit the block after the question was
//!   drawn is not in the list, so it gets nothing from this answer; the window asks it anew.
//! - **"For this chat" is each task's own grant**, held under that task's id and judged against
//!   that task's own folder, as a block's Allow is for one chat ([`crate::sandboxing`]). Every
//!   task is judged before anything is kept, so one that cannot be allowed allows none.
//! - **"For me" and "for the project" are kept once**, as one Allow keeps them, and every task
//!   listed is owed the restart that takes it.

use purlis_core::sandbox::grant::{self, Level, What};

use crate::planes::{PlaneId, Planes};
use crate::sandboxing::{Allowed, GrantLevel, GrantWhat};

/// How far up a lineage is followed before it is taken to reach nothing: far past any depth a
/// dispatch is allowed, so only a record that loops stops here.
const MOST_DEPTH: usize = 64;

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

/// What the window asks to allow, for which session's tasks and which tasks, at which level.
/// What is allowed is [`Doing::judge`]'s to say.
#[derive(Debug, Clone, Copy)]
pub struct Asked<'a> {
    pub session: u32,
    pub tasks: &'a [u32],
    pub level: GrantLevel,
}

/// What [`answered`] is told to do with the app, so it reads no chat store of its own.
pub struct Doing<'a> {
    /// Who asked whom ([`below`]).
    pub asker_of: AskerOf<'a>,
    /// What allowing the target for one task would grant, judged by the core
    /// ([`crate::sandboxing::judged`]).
    pub judge: &'a dyn Fn(u32) -> Result<(What, Level), String>,
    /// Keeps one grant for one task: audited, kept and the task owed its restart
    /// ([`crate::sandboxing::kept`]).
    pub keep: &'a mut dyn FnMut(u32, &What, Level) -> Result<(), String>,
    /// Owes one task a restart that tells it what it was allowed.
    pub owe: &'a dyn Fn(u32, String),
}

/// **The one answer to the tasks a Notice listed** ([module](self)): checked against the app's
/// record, every task judged, then kept for each task (this chat) or once (for me, for the
/// project) with each task owed its restart. Nothing is kept unless every check passed.
pub fn answered(asked: Asked<'_>, doing: Doing<'_>) -> Result<Allowed, String> {
    let Asked {
        session,
        tasks,
        level,
    } = asked;
    if tasks.is_empty() {
        return Err("No task was named, so nothing was allowed.".to_owned());
    }
    if tasks.contains(&session) {
        return Err(
            "Nothing was allowed: this answer is for the tasks the question listed, and a \
             session is not one of its own tasks. Answer the session's own block on it."
                .to_owned(),
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    if let Some(twice) = tasks.iter().find(|task| !seen.insert(**task)) {
        return Err(format!(
            "Nothing was allowed: chat {twice} was named twice."
        ));
    }
    if let Some(stranger) = tasks
        .iter()
        .find(|task| !below(doing.asker_of, session, **task))
    {
        return Err(format!(
            "Nothing was allowed: purlis has no record of chat {stranger} as a task of this \
             session, and one answer covers only the session's own tasks. Ask again from the \
             question on the session's tab."
        ));
    }
    let mut judged = Vec::with_capacity(tasks.len());
    for task in tasks {
        let (what, level) = (doing.judge)(*task)
            .map_err(|why| format!("Nothing was allowed: for chat {task}, {why}"))?;
        judged.push((*task, what, level));
    }
    let count = tasks.len();
    if Level::from(level) == Level::Chat {
        let mut allowed = Vec::new();
        for (task, what, level) in &judged {
            (doing.keep)(*task, what, *level).map_err(|why| {
                if allowed.is_empty() {
                    format!("Nothing was allowed: {why}")
                } else {
                    format!(
                        "It was allowed for chat {} only, and not for the rest: {why}",
                        numbers(&allowed)
                    )
                }
            })?;
            allowed.push(*task);
        }
        return Ok(Allowed {
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
        });
    }
    // Kept once: every task was judged to the same grant, or it is not one answer.
    let [(first, what, level), rest @ ..] = judged.as_slice() else {
        return Err("No task was named, so nothing was allowed.".to_owned());
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
        (doing.owe)(*task, told.clone());
    }
    Ok(Allowed {
        said: format!(
            "Allowed {}. The {count} {} restart on the same conversation once each one's turn \
             ends, and are told to retry.",
            level.said(),
            if count == 1 { "task" } else { "tasks" }
        ),
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

/// **Allow on the one question for several tasks of chat `session`** (#1508, V100-57): every
/// chat of `tasks` is a task below `session` by the app's own record, each is judged on its
/// own, and the answer is kept for each task alone (`chat`), or once for every chat of the
/// project on this machine (`you`) or for everyone in it (`project`), with each task owed a
/// restart. The window then restarts each once its turn has ended (`restart_chat`).
#[tauri::command]
#[specta::specta]
pub fn allow_sandbox_block_for_tasks(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    tasks: Vec<u32>,
    what: GrantWhat,
    target: String,
    level: GrantLevel,
) -> Result<Allowed, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    let chats = held.chats();
    let machine = purlis_core::sandbox::Machine::this();
    let at = crate::sandboxing::now_secs();
    let audit = |number: Option<u32>, audited: &grant::Audited<'_>| {
        held.hooks().record_grant(&root, number, audited)
    };
    answered(
        Asked {
            session,
            tasks: &tasks,
            level,
        },
        Doing {
            asker_of: &|chat| asker_in(chats, chat),
            judge: &|task| {
                crate::sandboxing::judged(
                    &root,
                    &machine,
                    chats.folder_of(task).as_deref(),
                    task,
                    (what, &target, level),
                )
            },
            keep: &mut |task, what, level| {
                crate::sandboxing::kept(&root, chats, task, (what, level), &audit, at)
            },
            owe: &|task, told| chats.owe_restart(task, told),
        },
    )
}

#[cfg(test)]
#[path = "taskblocks_tests.rs"]
mod tests;
