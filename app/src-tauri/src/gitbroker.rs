//! A clone or a worktree a chat asks the app to make for it (#1335, ADR 0067 §2): the app's
//! record of the chat, handed to `purlis_core::gitbroker`, which checks the ask against it and
//! runs the same core functions the terminal's commands run.
//!
//! And a commit in the branch folder a sandboxed chat stands in (#1055), handed to
//! `purlis_core::gitbroker::commit`.
//!
//! Answered on the listener's thread for this one connection, so a slow clone holds no other
//! chat's hooks or asks (D-6).

use purlis_core::gitbroker::Asker;
use purlis_core::hookwire::{Answer, CommitAsk, GitAsk, GitWork, Stage};

use crate::planes::Held;

/// The configuration git reads where the app runs it for a chat, or in a repository a chat
/// worked in: no global or system git config, only the operator's identity, read from their
/// global config before the call runs (D-1335-7).
///
/// A test build reads no identity either: a test never reads the developer's own git config.
/// The one call made under it that writes a commit is a chat's brokered commit (#1055), which
/// no test of this crate makes.
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

/// The app's own record of chat `chat`, or the answer where it has none open.
fn asker(held: &Held, chat: u32) -> Result<Asker, Answer> {
    let Some(open) = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == chat)
    else {
        return Err(Answer::No {
            why: format!("chat {chat} is not one this app has open"),
        });
    };
    Ok(Asker {
        chat,
        cwd: open.cwd,
        persona: open.persona,
        harnessed: open.harness.is_some(),
        unsandboxed: held.chats().unsandboxed(chat),
        config: held.config().map(std::path::Path::to_path_buf),
    })
}

/// Commits what `ask` names in the branch folder its chat stands in (#1055), or says why not.
/// The folder is this app's record of the chat; the ask names a message and paths.
pub fn commit(held: &Held, ask: &CommitAsk) -> Answer {
    let asker = match asker(held, ask.chat) {
        Ok(asker) => asker,
        Err(no) => return no,
    };
    let what = match &ask.stage {
        Stage::Tracked => "every tracked change".to_owned(),
        Stage::Paths(paths) => format!("{} path(s)", paths.len()),
    };
    // The person's name and email, read before the commit runs; nothing else of their
    // global or system git config reaches it.
    let identity = isolation();
    let answer = purlis_core::gitbroker::commit::answer(held.root(), &asker, ask, &identity);
    let folder = asker.cwd.as_deref().map_or_else(
        || "no recorded folder".to_owned(),
        |cwd| cwd.display().to_string(),
    );
    match &answer {
        Answer::Said { .. } => tracing::info!(
            "purlis: chat {} asked the app to commit {what} in {folder}; committed",
            ask.chat
        ),
        Answer::No { why } => tracing::info!(
            "purlis: chat {} asked the app to commit {what} in {folder}; refused: {why}",
            ask.chat
        ),
        _ => {}
    }
    answer
}

/// Runs `ask` for its chat, or says why not.
pub fn answer(held: &Held, ask: &GitAsk) -> Answer {
    let asker = match asker(held, ask.chat) {
        Ok(asker) => asker,
        Err(no) => return no,
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
