//! `charter session record|list|show`: the command line of [`purlis_core::sessionrecord`]
//! (SI-8, ADR 0064).
//!
//! `record` is the one way a session record is written, and the one thing that ends a Smart
//! close: once the record, the index and the workspace's pointer are on disk it tells the app,
//! over the chat's hook socket, which chat saved which record. It learns everything it writes
//! into the frontmatter from the chat's environment, the app's record of the chat and git —
//! never from the model, which gives only the title and the body.

use std::io::{IsTerminal, Read};
use std::path::Path;

use clap::Subcommand;
use purlis_core::hookwire::{self, CHAT_ENV, Report, SOCKET_ENV, SessionSaved};
use purlis_core::sessionrecord::{self, Facts, New, Refused, relay};

use crate::Here;
use crate::memory::Code;
use crate::voice;

/// `purlis session …`.
#[derive(Subcommand)]
pub enum SessionCommand {
    /// Write this chat's session record — a summary, never the transcript — into
    /// workspaces/<ws>/sessions/ (or the plane's sessions/ at the plane root), rebuild the
    /// index and workspace.md's ## Sessions line, then tell the app this chat's record is
    /// saved. The body is read from standard input and is exactly the sections ## Goal,
    /// ## Done, ## Decisions, ## Open and ## How to resume.
    Record {
        /// The record's title: one line, at most 120 characters.
        #[arg(long)]
        title: String,
        /// A piece this chat worked in, as <repo>/<piece>; repeat for each. purlis records
        /// the branch git has it on. The piece this command runs in is recorded without it.
        #[arg(long = "piece", value_name = "REPO/PIECE")]
        pieces: Vec<String>,
        /// The workspace (default: where this chat works — a workspace, or the plane root).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
        /// Pin the clock, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
    /// List the session records of a workspace (or of the plane root), newest first.
    List {
        /// The workspace (default: where this chat works).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
    /// Print one session record: its file name, or its path from the plane root.
    Show {
        /// A record's file name (YYYYMMDD-HHMMSS-<title>.md), or its plane-relative path
        /// (workspaces/<ws>/sessions/<file>, sessions/<file>).
        file: String,
        /// The workspace a bare file name is in (default: where this chat works).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
    },
}

pub fn run(here: &Here, command: SessionCommand) -> Result<Code, String> {
    match command {
        SessionCommand::Record {
            title,
            pieces,
            workspace,
            now,
        } => record(here, &title, &pieces, workspace.as_deref(), now.as_deref()),
        SessionCommand::List { workspace } => {
            let place = here.place(workspace.as_deref());
            let records = sessionrecord::list(here.plane.root(), &place);
            if records.is_empty() {
                voice::info(&format!("No session records at {}.", place.said()));
            }
            for listed in records {
                println!("{} · {} — {}", listed.when, listed.title, listed.shown);
            }
            Ok(0)
        }
        SessionCommand::Show { file, workspace } => {
            // A bare file name is in this chat's place; a path is read by the one reading of a
            // record's path there is (`sessionrecord::locate`), which the app's Sessions panel
            // and the briefing use too.
            let shown = if file.contains('/') || file.contains('\\') {
                sessionrecord::open(here.plane.root(), &file).map(|opened| opened.text)
            } else {
                sessionrecord::show(here.plane.root(), &here.place(workspace.as_deref()), &file)
            };
            match shown {
                Ok(text) => {
                    print!("{text}");
                    Ok(0)
                }
                Err(why) => {
                    voice::err(&why);
                    Ok(1)
                }
            }
        }
    }
}

fn record(
    here: &Here,
    title: &str,
    pieces: &[String],
    workspace: Option<&str>,
    now: Option<&str>,
) -> Result<Code, String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        voice::err(
            "the record's body is read from standard input — pipe it in, or end it with a \
             here-document",
        );
        return Ok(2);
    }
    let mut body = String::new();
    stdin
        .lock()
        .read_to_string(&mut body)
        .map_err(|e| format!("could not read the record from standard input: {e}"))?;
    match write(here, title, &body, pieces, workspace, now) {
        Ok(saved) => {
            for warning in &saved.warnings {
                voice::warn(warning);
            }
            voice::ok(&format!("Session record → {}", saved.shown));
            println!("{}", saved.shown);
            if saved.tab_warns {
                voice::warn(&saved.tab);
            } else {
                voice::info(&saved.tab);
            }
            Ok(0)
        }
        Err(NotWritten::Refused(why)) => {
            voice::err(&why);
            Ok(2)
        }
        Err(NotWritten::Failed(why)) => Err(why),
    }
}

/// A session record written, by the app or by this process, and what becomes of the tab.
pub struct Saved {
    /// The record, plane-relative.
    pub shown: String,
    /// What did not follow it ([`sessionrecord::Recorded::warnings`]).
    pub warnings: Vec<String>,
    /// What becomes of the chat's tab, in a sentence the chat relays.
    pub tab: String,
    /// Whether that sentence is a warning: the tab is left for the operator because something
    /// failed, not because nobody asked for it to close.
    pub tab_warns: bool,
}

/// Why no record was written.
pub enum NotWritten {
    /// The title, body, place or a piece is not a record's: the sentence says what to fix.
    Refused(String),
    /// The disk refused, or the clock could not be read.
    Failed(String),
}

/// Writes this chat's session record: the one operation `purlis session record` and the MCP
/// server's `session_record` tool share (ADR 0067 §2, "two entrances, one operation").
///
/// **In a chat the app started, the app writes it** ([`forwarded`], #1332): a brokered write,
/// so the chat's sandbox never has to reach the project's files, and the record's facts are
/// the app's record of the chat. Where no app takes the ask (none listening, one older than
/// the ask, or a socket the sandbox refuses), or where `-w` or the test clock names what the
/// app's record would not, this process writes it, as it always has, and tells the app after.
pub fn write(
    here: &Here,
    title: &str,
    body: &str,
    pieces: &[String],
    workspace: Option<&str>,
    now: Option<&str>,
) -> Result<Saved, NotWritten> {
    if workspace.is_none() && now.is_none() {
        match forwarded(here, title, body, pieces) {
            Forwarded::Written(saved) => return Ok(saved),
            Forwarded::Refused(why) => return Err(NotWritten::Refused(why)),
            Forwarded::Unsure(why) => return Err(NotWritten::Failed(why)),
            Forwarded::NotTaken => {}
        }
    }
    let root = here.plane.root();
    let place = here.place(workspace);
    let touched = sessionrecord::touched(root, Some(&here.cwd), &place, pieces)
        .map_err(NotWritten::Refused)?;
    let chat = chat_number();
    let facts = Facts {
        place,
        at: crate::memory::stamp(now).map_err(NotWritten::Failed)?,
        chat: chat.map(|n| sessionrecord::chat_facts(root, n)),
        persona: here.active_persona(None),
        pieces: touched,
    };
    let recorded = match sessionrecord::record(
        root,
        &New {
            title,
            body,
            facts: &facts,
        },
    ) {
        Ok(recorded) => recorded,
        Err(Refused::Io(e)) => {
            return Err(NotWritten::Failed(format!(
                "could not write the session record: {e}"
            )));
        }
        Err(refused) => return Err(NotWritten::Refused(refused.to_string())),
    };
    let conversation = facts.chat.as_ref().and_then(|c| c.conversation.clone());
    let (tab, tab_warns) = tell_the_app(here, chat, conversation, &recorded.path);
    Ok(Saved {
        shown: recorded.shown,
        warnings: recorded.warnings,
        tab,
        tab_warns,
    })
}

/// What the app made of a record handed to it.
enum Forwarded {
    Written(Saved),
    /// The app read it and will not write it: the sentence says why.
    Refused(String),
    /// No app took it, so this process writes it.
    NotTaken,
    /// The app took it and did not answer: it may be written, so nothing is written here.
    Unsure(String),
}

/// How long the app may take to write a record and answer: a file, an index and a pointer.
const A_RECORD_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(10);

/// What becomes of the tab when nobody started a smart close for the chat: the sentence the
/// skill relays to the operator.
pub const NO_PASS: &str = "No smart close was started for this chat (the tab's Smart close, or \
                           typing /smart-close), so its tab will not close by itself. Close it \
                           when you are done.";

/// Hands the record to the app that started this chat, over the chat's hook socket (#1332).
fn forwarded(here: &Here, title: &str, body: &str, pieces: &[String]) -> Forwarded {
    let socket = purlis_core::envvar::var_os(SOCKET_ENV).filter(|s| !s.is_empty());
    let (Some(socket), Some(chat)) = (socket, chat_number()) else {
        return Forwarded::NotTaken;
    };
    let Ok(mut asking) = hookwire::Asking::on(Path::new(&socket), hookwire::ChatToken::from_env())
    else {
        return Forwarded::NotTaken;
    };
    let ask = hookwire::Ask::SessionRecord(Box::new(hookwire::RecordAsk {
        chat,
        title: title.to_owned(),
        body: body.to_owned(),
        pieces: pieces.to_vec(),
        cwd: Some(here.cwd.clone()),
    }));
    answered(asking.ask(&ask, A_RECORD_TAKES_AT_MOST))
}

/// What the app's answer, or the way the asking failed, makes of a record handed to it.
///
/// **Only a connection the app ended at once is not taken.** That is an app older than the
/// ask, which drops a line it cannot read, and then this process writes the record itself. A
/// connection that went quiet is an app that may be writing it now: writing it here as well
/// would leave two records, so the command says so and writes nothing (#1332).
fn answered(answer: std::io::Result<hookwire::Answer>) -> Forwarded {
    use std::io::ErrorKind;
    match answer {
        Ok(hookwire::Answer::Recorded {
            record,
            closes,
            warnings,
        }) => Forwarded::Written(Saved {
            shown: record,
            warnings,
            tab: if closes {
                "The app wrote it, and closes this tab when this turn ends.".to_owned()
            } else {
                format!("The app wrote it. {NO_PASS}")
            },
            tab_warns: false,
        }),
        // An app with nothing that answers asks: a test's, or one that is closing.
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
            "the app took this record and did not answer ({e}), so it may already be written. \
             Nothing was written again here: `purlis session list` shows whether it is there."
        )),
    }
}

/// The app's number for the chat this runs in: `$CHARTER_CHAT` where hooks report, else
/// `$CHARTER_SESSION_ID`, which the app sets in every chat it starts to the same number. A value
/// that is not a number (a session id from outside the app) is no chat.
fn chat_number() -> Option<u32> {
    [CHAT_ENV, purlis_core::active::SESSION_ID_ENV]
        .into_iter()
        .filter_map(purlis_core::envvar::var)
        .find_map(|n| n.trim().parse::<u32>().ok().filter(|n| *n > 0))
}

/// Tells the app this chat's record is saved, and answers what becomes of the tab, as a
/// sentence and whether it is a warning: the record is written whatever happens here, and a
/// tab that nothing will close is the operator's to close.
///
/// **A line the socket refused is left for the chat's next `Stop` hook** (#517,
/// `sessionrecord::relay`): a harness can run this command in a sandbox that refuses the
/// connect — Codex's default one does — and runs its hooks outside it. So is one with no
/// socket to send to in a chat the app started: Codex hands an MCP server the chat's number
/// and never its socket. It is left where the chat works (`Here::place` with no `-w`), which
/// is where the sandbox lets it write and where the hook looks.
fn tell_the_app(
    here: &Here,
    chat: Option<u32>,
    conversation: Option<String>,
    path: &Path,
) -> (String, bool) {
    let Some(chat) = chat else {
        return (
            "This chat was not started by the app (no $PURLIS_HOOK_SOCKET and $PURLIS_CHAT), \
             so its tab will not close by itself — close it when you are done."
                .to_owned(),
            false,
        );
    };
    let saved = SessionSaved {
        chat,
        session_saved: path.to_path_buf(),
    };
    let not_heard = match purlis_core::envvar::var_os(SOCKET_ENV).filter(|s| !s.is_empty()) {
        Some(socket) => {
            let token = hookwire::ChatToken::from_env();
            match hookwire::tell_saved(Path::new(&socket), token.as_ref(), &saved) {
                Ok(()) => {
                    return (
                        "Told the app: a Smart close waiting on this record closes the tab."
                            .to_owned(),
                        false,
                    );
                }
                Err(e) => e.to_string(),
            }
        }
        None => "no socket to tell it on".to_owned(),
    };
    let left = relay::Marker {
        chat,
        conversation,
        session_saved: saved.session_saved,
        at: unix_now(),
    };
    match relay::leave(here.plane.root(), &here.place(None), &left) {
        Ok(_) => (
            format!(
                "The app did not hear it ({not_heard}), so this chat's hook tells it when this \
                 turn ends. If the tab is still open after that, close it yourself."
            ),
            false,
        ),
        Err(kept) => (
            format!(
                "The app did not hear it ({not_heard}), and purlis could not leave it for this \
                 chat's hook ({kept}), so the tab will not close by itself — close it yourself \
                 when you are done."
            ),
            true,
        ),
    }
}

/// `purlis hook stop` in a chat the app started: the saved-record line `purlis session
/// record` left because the socket refused it, sent now (`sessionrecord::relay::take`). The
/// hook runs outside the harness's sandbox, so its connect gets through where the command's
/// did not. Silent where there is nothing to send, as a reporting hook is.
pub fn pass_on_at_stop(socket: &Path, report: &Report) {
    let Ok(here) = Here::read() else { return };
    let Some(saved) = relay::take(
        here.plane.root(),
        &here.place(None),
        report.chat,
        &report.conversation,
        unix_now(),
    ) else {
        return;
    };
    if let Err(why) = hookwire::tell_saved(socket, hookwire::ChatToken::from_env().as_ref(), &saved)
    {
        // Where a reporting hook's failures go: the harness's own log (`main::hook`).
        eprintln!("purlis: the app did not take this chat's saved session record ({why})");
    }
}

/// Seconds since the Unix epoch, as a marker records when it was left.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind};

    use super::*;

    #[test]
    fn an_app_that_went_quiet_may_have_written_the_record_so_it_is_not_written_again() {
        for kind in [ErrorKind::TimedOut, ErrorKind::WouldBlock] {
            let Forwarded::Unsure(why) = answered(Err(Error::new(kind, "slow"))) else {
                panic!("a {kind:?} was taken as no app");
            };
            assert!(why.contains("may already be written"), "{why}");
            assert!(why.contains("purlis session list"), "{why}");
        }
    }

    #[test]
    fn an_app_that_ended_the_connection_at_once_did_not_take_the_record() {
        for kind in [
            ErrorKind::UnexpectedEof,
            ErrorKind::BrokenPipe,
            ErrorKind::ConnectionReset,
        ] {
            assert!(
                matches!(answered(Err(Error::new(kind, "gone"))), Forwarded::NotTaken),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_written_record_under_a_pass_says_the_tab_closes_when_the_turn_ends() {
        let Forwarded::Written(saved) = answered(Ok(hookwire::Answer::Recorded {
            record: "sessions/r.md".to_owned(),
            closes: true,
            warnings: Vec::new(),
        })) else {
            panic!("not written");
        };
        assert!(saved.tab.contains("when this turn ends"), "{}", saved.tab);
    }
}
