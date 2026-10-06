//! Brokered writes from the command line (ADR 0067 §2, #1333): inside a chat the app started, a
//! `purlis` command that writes the project's own files hands the write to the app over the
//! chat's hook socket, and prints what the app wrote as it would have printed its own write.
//!
//! The rules for when to hand it over, and when to write here instead, are the session
//! record's (#1332), and this module is where both read them:
//!
//! - **No socket, no chat number, or a socket nothing listens on:** no app takes it, and the
//!   command writes it itself, as it always has.
//! - **The app ended the connection at once:** an app older than the ask, which drops a line
//!   it cannot read. The command writes it itself.
//! - **The app went quiet:** it may be writing it now. The command says so and writes nothing,
//!   so nothing is ever written twice.

use std::path::Path;

use purlis_core::brokered::Write;
use purlis_core::hookwire::{self, Answer, Ask, SOCKET_ENV};

/// What became of a write handed to the app.
pub enum Forwarded {
    /// The app wrote it: the workspace or persona it went to, and the file, project-relative.
    Written { to: String, path: String },
    /// The app read it and will not write it: the sentence says why.
    Refused(String),
    /// No app took it. A command, which runs inside the chat's sandbox, writes it itself; the
    /// MCP server, which does not, writes it only where [`NotTaken::may_write_outside`] says.
    NotTaken(NotTaken),
    /// The app took it and did not answer: it may be written, so nothing is written here.
    Unsure(String),
}

/// Why no app took a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotTaken {
    /// This is no chat the app started: no chat number.
    NotAChat,
    /// A chat the app started, whose harness gave this process no connection to the app: the
    /// MCP server under Codex, which hands it only [`purlis_core::chattools::SCOPE_ENV`].
    NoConnection,
    /// The app's socket is not there, or nothing listens on it: the app has gone.
    AppGone,
    /// The app ended the connection without answering: an app older than the ask.
    Dropped,
}

impl NotTaken {
    /// Whether a process running outside the chat's sandbox may write it itself: only where
    /// there is no app that could have (#1333). Anything else would be a write no sandbox
    /// bounds, no rate holds and the trace does not credit.
    pub fn may_write_outside(self) -> bool {
        matches!(self, Self::NotAChat | Self::AppGone)
    }

    /// The sentence a process outside the sandbox refuses with, where it may not write:
    /// `command` is the one to run in the chat instead.
    pub fn refusal(self, command: &str) -> String {
        match self {
            Self::NoConnection => format!(
                "this harness does not hand purlis's tools the chat's connection to the app, so \
                 the app cannot write it for this chat: run `{command}` in the chat instead"
            ),
            _ => format!(
                "the app that started this chat did not take the write, so nothing was written: \
                 run `{command}` in the chat instead"
            ),
        }
    }
}

/// How long the app may take to write one memory or todo and answer.
const A_WRITE_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(10);

/// Hands `write` to the app that started this chat, if one did.
pub fn forwarded(write: Write) -> Forwarded {
    let answer = match asked(
        |chat| {
            Ask::Write(Box::new(hookwire::WriteAsk {
                chat,
                write: write.clone(),
            }))
        },
        A_WRITE_TAKES_AT_MOST,
    ) {
        Ok(answer) => answer,
        Err(not) => return Forwarded::NotTaken(not),
    };
    match answer {
        Ok(Answer::Written { to, path }) => Forwarded::Written { to, path },
        Ok(Answer::No { why }) if why == hookwire::NOTHING_ANSWERS => {
            Forwarded::NotTaken(NotTaken::Dropped)
        }
        Ok(Answer::No { why }) => Forwarded::Refused(why),
        Ok(_) => Forwarded::NotTaken(NotTaken::Dropped),
        Err(e) if not_taken(&e) => Forwarded::NotTaken(NotTaken::Dropped),
        Err(e) => Forwarded::Unsure(format!(
            "the app took this write and did not answer ({e}), so it may already be written. \
             Nothing was written again here: list them to see whether it is there."
        )),
    }
}

/// The app's answer to the ask `make` builds for this chat's number, or why no app can be
/// asked.
pub fn asked(
    make: impl FnOnce(u32) -> Ask,
    within: std::time::Duration,
) -> Result<std::io::Result<Answer>, NotTaken> {
    let chat = crate::session::chat_number().ok_or(NotTaken::NotAChat)?;
    let socket = purlis_core::envvar::var_os(SOCKET_ENV)
        .filter(|s| !s.is_empty())
        .ok_or(NotTaken::NoConnection)?;
    let mut asking = hookwire::Asking::on(Path::new(&socket), hookwire::ChatToken::from_env())
        .map_err(|e| {
            use std::io::ErrorKind;
            if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) {
                NotTaken::AppGone
            } else {
                NotTaken::Dropped
            }
        })?;
    Ok(asking.ask(&make(chat), within))
}

/// Whether asking failed the way an app that never took the ask fails: it ended the
/// connection at once.
pub fn not_taken(e: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        e.kind(),
        ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe | ErrorKind::ConnectionReset
    )
}
