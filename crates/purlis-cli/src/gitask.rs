//! `purlis clone` and `purlis worktree add` in a chat the app started: the work handed to the
//! app over the chat's hook socket, which runs the same core functions and answers every line
//! they said (#1335, ADR 0067 §2). A sandboxed chat may not write a clone's `.git/config`, its
//! hooks or the editor settings a checkout carries; the app does.
//!
//! **Forwarded whenever the command runs in a chat the app started** (`$PURLIS_HOOK_SOCKET` and
//! the chat's number), sandboxed or not, so a chat gets one behaviour (D-2). Outside a chat, or
//! where no app takes the ask, the command runs here as it always has.

use std::path::Path;
use std::time::Duration;

use purlis_core::gitbroker::commit as broker;
use purlis_core::hookwire::{self, CommitAsk, GitAsk, GitWork, SOCKET_ENV, Stage};
use purlis_core::repocmd::Say;
use purlis_core::worktree::git;

/// What became of a git action handed to the app.
pub enum Forwarded {
    /// The app ran it: the lines it said, to print as this process would have, and its status.
    Said { lines: Vec<Say>, code: u8 },
    /// The app read it and will not run it: the sentence says why.
    Refused(String),
    /// No app took it, so this process runs it.
    NotTaken,
    /// The app took it and did not answer in time: it may have run, so nothing runs here.
    Unsure(String),
}

/// How long the app may take to clone `repos` repos: git's own limit for each round of
/// [`purlis_core::repocmd::clone::WORKERS`] clones at once, and one more for the wiring and the
/// manifest after them (D-6). Above git's limit, so the asker never gives up on a clone git is
/// still allowed to finish.
pub fn a_clone_takes_at_most(repos: usize) -> Duration {
    let rounds = repos.max(1).div_ceil(purlis_core::repocmd::clone::WORKERS);
    git::NETWORK * u32::try_from(rounds + 1).unwrap_or(u32::MAX)
}

/// How long the app may take to cut a worktree: several local git calls, each held to
/// [`git::READ`] (D-6).
pub const A_WORKTREE_TAKES_AT_MOST: Duration = Duration::from_secs(4 * git::READ.as_secs());

/// Hands `work` in `workspace` to the app that started this chat, or answers
/// [`Forwarded::NotTaken`] where there is none.
pub fn forwarded(workspace: &str, work: GitWork, within: Duration) -> Forwarded {
    asked(
        |chat| {
            hookwire::Ask::Git(Box::new(GitAsk {
                chat,
                workspace: workspace.to_owned(),
                work,
            }))
        },
        within,
    )
}

/// Hands the app that started this chat the ask `of` makes for the chat's number, or answers
/// [`Forwarded::NotTaken`] where there is none.
fn asked(of: impl FnOnce(u32) -> hookwire::Ask, within: Duration) -> Forwarded {
    let socket = purlis_core::envvar::var_os(SOCKET_ENV).filter(|s| !s.is_empty());
    let (Some(socket), Some(chat)) = (socket, crate::session::chat_number()) else {
        return Forwarded::NotTaken;
    };
    let Ok(mut asking) = hookwire::Asking::on(Path::new(&socket), hookwire::ChatToken::from_env())
    else {
        return Forwarded::NotTaken;
    };
    answered(asking.ask(&of(chat), within))
}

/// How long the app may take to commit: staging and the commit by their own deadlines, and a
/// handful of local git calls and the scan, each held to [`git::READ`]. Above what the app's
/// git may take, so the asker never gives up on a commit git is still allowed to finish.
pub const A_COMMIT_TAKES_AT_MOST: Duration = Duration::from_secs(
    broker::STAGING.as_secs() + broker::COMMITTING.as_secs() + 12 * git::READ.as_secs(),
);

/// What `purlis worktree commit` was asked, as the ask it sends, or the sentence that refuses
/// the spelling before anything is sent.
pub fn commit_ask(
    message: Option<String>,
    all: bool,
    amend: bool,
    paths: Vec<String>,
) -> Result<(String, Stage), String> {
    if amend {
        return Err(broker::COMMITS_ONLY.to_owned());
    }
    let message = message.ok_or("give the commit message with -m")?;
    broker::message_ok(&message)?;
    let stage = match (all, paths.is_empty()) {
        (true, true) => Stage::Tracked,
        (false, false) => {
            broker::paths_ok(&paths)?;
            Stage::Paths(paths)
        }
        (true, false) => {
            return Err(
                "--all stages every change to a tracked file: pass it, or name paths, not both"
                    .to_owned(),
            );
        }
        (false, true) => {
            return Err(
                "name what to commit: paths, or --all for every change to a tracked file"
                    .to_owned(),
            );
        }
    };
    Ok((message, stage))
}

/// `purlis worktree commit`: hands the commit to the app that started this sandboxed chat
/// (#1055), prints what it answers, and answers the exit status. Nothing is committed by this
/// process: a chat that is not sandboxed is told to use git.
pub fn commit(
    message: Option<String>,
    all: bool,
    amend: bool,
    paths: Vec<String>,
    say: &mut dyn FnMut(Say),
) -> u8 {
    let (message, stage) = match commit_ask(message, all, amend, paths) {
        Ok(ask) => ask,
        Err(why) => {
            say(Say::Fail(why));
            return 2;
        }
    };
    if !purlis_core::sandbox::chat_is_sandboxed() {
        say(Say::Fail(broker::PLAIN_GIT.to_owned()));
        return 1;
    }
    let forwarded = asked(
        |chat| {
            hookwire::Ask::Commit(Box::new(CommitAsk {
                chat,
                message,
                stage,
            }))
        },
        A_COMMIT_TAKES_AT_MOST,
    );
    told(forwarded, say).unwrap_or_else(|| {
        say(Say::Fail(
            "the app that started this chat did not take the commit: it may be older than \
             this command. Nothing was committed. Ask the person to update purlis, or to \
             commit from their own terminal"
                .to_owned(),
        ));
        1
    })
}

/// What the app's answer, or the way asking failed, makes of a git action handed to it.
///
/// `purlis session record`'s rule (#1332): **only a connection the app ended at once is not
/// taken** — an app older than the ask drops a line it cannot read — and then this process
/// runs the command itself. A connection that went quiet is an app that may be cloning now,
/// and cloning here as well would race it into the same directory, so nothing runs here.
fn answered(answer: std::io::Result<hookwire::Answer>) -> Forwarded {
    use std::io::ErrorKind;
    match answer {
        Ok(hookwire::Answer::Said { lines, code }) => Forwarded::Said { lines, code },
        Ok(hookwire::Answer::No { why }) if why == hookwire::NOTHING_ANSWERS => Forwarded::NotTaken,
        Ok(hookwire::Answer::No { why }) => Forwarded::Refused(why),
        Ok(_) => Forwarded::NotTaken,
        Err(e)
            if matches!(
                e.kind(),
                ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe | ErrorKind::ConnectionReset
            ) =>
        {
            Forwarded::NotTaken
        }
        Err(e) => Forwarded::Unsure(format!(
            "the app took this and did not answer ({e}), so it may still be running it. Nothing \
             was run again here: `purlis status` shows what is in the workspace now."
        )),
    }
}

/// Prints what the app answered with `say`, the printer this command's own run uses, and
/// answers the exit status. A refusal and an unsure answer are failures said in its voice.
pub fn told(forwarded: Forwarded, say: &mut dyn FnMut(Say)) -> Option<u8> {
    match forwarded {
        Forwarded::Said { lines, code } => {
            for line in lines {
                say(line);
            }
            Some(code)
        }
        Forwarded::Refused(why) => {
            say(Say::Fail(format!(
                "the app did not run this for the chat: {why}"
            )));
            Some(1)
        }
        Forwarded::Unsure(why) => {
            say(Say::Fail(why));
            Some(1)
        }
        Forwarded::NotTaken => None,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind};

    use super::*;

    #[test]
    fn an_app_that_ends_the_connection_at_once_did_not_take_it_and_a_quiet_one_may_have() {
        for kind in [
            ErrorKind::UnexpectedEof,
            ErrorKind::BrokenPipe,
            ErrorKind::ConnectionReset,
        ] {
            assert!(matches!(
                answered(Err(Error::from(kind))),
                Forwarded::NotTaken
            ));
        }
        assert!(matches!(
            answered(Err(Error::from(ErrorKind::WouldBlock))),
            Forwarded::Unsure(_)
        ));
        assert!(matches!(
            answered(Ok(hookwire::Answer::No {
                why: hookwire::NOTHING_ANSWERS.to_owned()
            })),
            Forwarded::NotTaken
        ));
    }

    #[test]
    fn a_commit_is_asked_for_by_message_and_paths_and_other_spellings_are_refused() {
        let paths = |of: &[&str]| of.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            commit_ask(Some("one".to_owned()), true, false, Vec::new()),
            Ok(("one".to_owned(), Stage::Tracked))
        );
        assert_eq!(
            commit_ask(Some("one".to_owned()), false, false, paths(&["a", "-n"])),
            Ok(("one".to_owned(), Stage::Paths(paths(&["a", "-n"]))))
        );
        // An amend is refused in the sentence that names everything this never does.
        let amend = commit_ask(Some("one".to_owned()), true, true, Vec::new()).unwrap_err();
        assert_eq!(amend, broker::COMMITS_ONLY);
        for (message, all, of) in [
            (None, true, vec![]),
            (Some(" "), true, vec![]),
            (Some("one"), false, vec![]),
            (Some("one"), true, vec!["a"]),
            (Some("one"), false, vec!["../a"]),
            (Some("one"), false, vec![".git/config"]),
        ] {
            assert!(
                commit_ask(message.map(str::to_owned), all, false, paths(&of)).is_err(),
                "{message:?} {all} {of:?}"
            );
        }
    }

    #[test]
    fn the_command_has_no_spelling_for_a_reset_or_a_push() {
        use clap::Parser as _;
        for words in [
            vec!["purlis", "worktree", "reset"],
            vec!["purlis", "worktree", "push"],
            vec![
                "purlis", "worktree", "commit", "-m", "x", "--all", "--reset",
            ],
            vec!["purlis", "worktree", "commit", "-m", "x", "--all", "--push"],
            vec![
                "purlis",
                "worktree",
                "commit",
                "-m",
                "x",
                "--all",
                "--no-verify",
            ],
            vec![
                "purlis", "worktree", "commit", "-m", "x", "--all", "--branch", "main",
            ],
            vec![
                "purlis", "worktree", "commit", "-m", "x", "--all", "--author", "a",
            ],
        ] {
            assert!(crate::Cli::try_parse_from(&words).is_err(), "{words:?}");
        }
        assert!(
            crate::Cli::try_parse_from(["purlis", "worktree", "commit", "-m", "x", "--", "-n"])
                .is_ok()
        );
    }

    #[test]
    fn the_wait_is_longer_than_git_may_take() {
        assert!(a_clone_takes_at_most(1) > git::NETWORK);
        assert!(a_clone_takes_at_most(purlis_core::repocmd::clone::WORKERS + 1) > git::NETWORK * 2);
        assert!(A_WORKTREE_TAKES_AT_MOST > git::READ);
        assert!(A_COMMIT_TAKES_AT_MOST > broker::STAGING + broker::COMMITTING);
    }

    #[test]
    fn what_the_app_said_is_printed_line_for_line_and_its_status_is_the_commands() {
        let mut printed = Vec::new();
        let code = told(
            Forwarded::Said {
                lines: vec![Say::Info("a".to_owned()), Say::Fail("b".to_owned())],
                code: 2,
            },
            &mut |line| printed.push(line),
        );
        assert_eq!(code, Some(2));
        assert_eq!(
            printed,
            [Say::Info("a".to_owned()), Say::Fail("b".to_owned())]
        );
        assert_eq!(told(Forwarded::NotTaken, &mut |_| {}), None);
    }
}
