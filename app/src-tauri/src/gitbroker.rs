//! A clone or a worktree a chat asks the app to make for it (#1335, ADR 0067 §2): the app's
//! record of the chat, handed to `purlis_core::gitbroker`, which checks the ask against it and
//! runs the same core functions the terminal's commands run.
//!
//! Answered on the listener's thread for this one connection, so a slow clone holds no other
//! chat's hooks or asks (D-6).

use purlis_core::gitbroker::Asker;
use purlis_core::hookwire::{Answer, GitAsk, GitWork};

use crate::planes::Held;

/// The configuration git reads where the app runs it for a chat, or in a repository a chat
/// worked in: no global or system git config, only the operator's identity, read from their
/// global config before the call runs (D-1335-7).
///
/// A test build reads no identity either: a test never reads the developer's own git config,
/// and none of the calls made under it writes a commit.
pub(crate) fn isolation() -> purlis_core::worktree::git::Isolated {
    #[cfg(not(test))]
    {
        purlis_core::worktree::git::Isolated::operators()
    }
    #[cfg(test)]
    {
        purlis_core::worktree::git::Isolated::default()
    }
}

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
        config: held.config().map(std::path::Path::to_path_buf),
    };
    let what = match &ask.work {
        GitWork::Clone { repos } => format!("clone {}", repos.join(" ")),
        GitWork::WorktreeAdd { repo, piece, .. } => format!("worktree add {repo} {piece}"),
    };
    let isolation = isolation();
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
