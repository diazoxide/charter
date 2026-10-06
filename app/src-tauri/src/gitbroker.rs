//! A clone or a worktree a chat asks the app to make for it (#1335, ADR 0067 §2): the app's
//! record of the chat, handed to `purlis_core::gitbroker`, which checks the ask against it and
//! runs the same core functions the terminal's commands run.
//!
//! Answered on the listener's thread for this one connection, so a slow clone holds no other
//! chat's hooks or asks (D-6).

use purlis_core::gitbroker::Asker;
use purlis_core::hookwire::{Answer, GitAsk, GitWork};

use crate::planes::Held;

/// Runs `ask` for its chat, or says why not.
pub fn answer(held: &Held, ask: &GitAsk) -> Answer {
    let Some(open) = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == ask.chat)
    else {
        return Answer::No {
            why: format!("chat {} is not one this app has open", ask.chat),
        };
    };
    let asker = Asker {
        chat: ask.chat,
        cwd: open.cwd,
        persona: open.persona,
        harnessed: open.harness.is_some(),
        unsandboxed: held.chats().unsandboxed(ask.chat),
    };
    let what = match &ask.work {
        GitWork::Clone { repos } => format!("clone {}", repos.join(" ")),
        GitWork::WorktreeAdd { repo, piece, .. } => format!("worktree add {repo} {piece}"),
    };
    // No global or system git config reaches a brokered call, only the operator's identity,
    // read from their global config before it runs (D-1335-7).
    let isolation = purlis_core::worktree::git::Isolated::operators();
    let answer =
        purlis_core::gitbroker::answer(held.root(), &asker, ask, &isolation, chrono::Utc::now());
    match &answer {
        Answer::Said { code, .. } => tracing::info!(
            "purlis: chat {} asked the app to {what} in workspace '{}'; it ran, exit {code}",
            ask.chat,
            ask.workspace
        ),
        Answer::No { why } => tracing::info!(
            "purlis: chat {} asked the app to {what} in workspace '{}'; refused: {why}",
            ask.chat,
            ask.workspace
        ),
        _ => {}
    }
    answer
}
