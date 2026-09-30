//! `charter session record|list|show`: the command line of [`charter_core::sessionrecord`]
//! (SI-8, ADR 0064).
//!
//! `record` is the one way a session record is written, and the one thing that ends a Smart
//! close: once the record, the index and the workspace's pointer are on disk it tells the app,
//! over the chat's hook socket, which chat saved which record. It learns everything it writes
//! into the frontmatter from the chat's environment, the app's record of the chat and git —
//! never from the model, which gives only the title and the body.

use std::io::{IsTerminal, Read};
use std::path::Path;

use charter_core::active::Place;
use charter_core::hookwire::{self, CHAT_ENV, Report, SOCKET_ENV, SessionSaved};
use charter_core::sessionrecord::{self, Facts, New, Refused, Touched, relay};
use clap::Subcommand;

use crate::Here;
use crate::memory::Code;
use crate::voice;

/// `charter session …`.
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
        /// A piece this chat worked in, as <repo>/<piece>; repeat for each. charter records
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
    let root = here.plane.root();
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
    let place = here.place(workspace);
    let touched = match touched(root, &here.cwd, &place, pieces) {
        Ok(touched) => touched,
        Err(why) => {
            voice::err(&why);
            return Ok(2);
        }
    };
    let chat = chat_number();
    let facts = Facts {
        place,
        at: crate::memory::stamp(now)?,
        chat: chat.map(|n| sessionrecord::chat_facts(root, n)),
        persona: here.active_persona(None),
        pieces: touched,
    };
    let recorded = match sessionrecord::record(
        root,
        &New {
            title,
            body: &body,
            facts: &facts,
        },
    ) {
        Ok(recorded) => recorded,
        Err(Refused::Io(e)) => return Err(format!("could not write the session record: {e}")),
        Err(refused) => {
            voice::err(&refused.to_string());
            return Ok(2);
        }
    };
    for warning in &recorded.warnings {
        voice::warn(warning);
    }
    voice::ok(&format!("Session record → {}", recorded.shown));
    println!("{}", recorded.shown);
    let conversation = facts.chat.as_ref().and_then(|c| c.conversation.clone());
    tell_the_app(here, chat, conversation, &recorded.path);
    Ok(0)
}

/// The pieces this chat worked in: the one it stands in, and each `--piece`, as git reports
/// them. A `--piece` git does not report in this workspace is refused, never recorded.
fn touched(
    root: &Path,
    cwd: &Path,
    place: &Place,
    pieces: &[String],
) -> Result<Vec<Touched>, String> {
    let mut out: Vec<Touched> = Vec::new();
    let mut add = |ws: &str, repo: &str, piece: &str| -> Result<(), String> {
        if out.iter().any(|t| t.repo == repo && t.piece == piece) {
            return Ok(());
        }
        let listed = charter_core::worktree::list(root, ws, repo)
            .map_err(|e| format!("--piece {repo}/{piece}: {e}"))?;
        let Some(found) = listed.into_iter().find(|p| p.piece == piece) else {
            return Err(format!(
                "--piece {repo}/{piece}: git reports no such piece of {repo} in workspace {ws}"
            ));
        };
        out.push(Touched {
            repo: repo.to_owned(),
            piece: piece.to_owned(),
            branch: found.branch,
        });
        Ok(())
    };
    let ws = match place {
        Place::Workspace(ws) => Some(ws.as_str()),
        Place::PlaneRoot => None,
    };
    if let (Some(ws), Some((at, repo, Some(piece)))) =
        (ws, charter_core::pieces::tree_at(root, cwd))
        && at == ws
    {
        // Where this command runs is a fact; one git will not describe is left out rather
        // than refused, because nobody named it.
        let _ = add(ws, &repo, &piece);
    }
    for named in pieces {
        let Some(ws) = ws else {
            return Err(format!(
                "--piece {named}: the plane root has no pieces; name the workspace with -w"
            ));
        };
        let Some((repo, piece)) = named.split_once('/') else {
            return Err(format!("--piece {named}: name it as <repo>/<piece>"));
        };
        add(ws, repo, piece)?;
    }
    Ok(out)
}

/// The app's number for the chat this runs in: `$CHARTER_CHAT` where hooks report, else
/// `$CHARTER_SESSION_ID`, which the app sets in every chat it starts to the same number. A value
/// that is not a number (a session id from outside the app) is no chat.
fn chat_number() -> Option<u32> {
    [CHAT_ENV, charter_core::active::SESSION_ID_ENV]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find_map(|n| n.trim().parse::<u32>().ok().filter(|n| *n > 0))
}

/// Tells the app this chat's record is saved, and says so either way: the record is written
/// whatever happens here, and a tab that nothing will close is the operator's to close.
///
/// **A line the socket refused is left for the chat's next `Stop` hook** (#517,
/// `sessionrecord::relay`): a harness can run this command in a sandbox that refuses the
/// connect — Codex's default one does — and runs its hooks outside it. It is left where the
/// chat works (`Here::place` with no `-w`), which is where the sandbox lets it write and where
/// the hook looks.
fn tell_the_app(here: &Here, chat: Option<u32>, conversation: Option<String>, path: &Path) {
    let socket = std::env::var_os(SOCKET_ENV).filter(|s| !s.is_empty());
    let (Some(socket), Some(chat)) = (socket, chat) else {
        voice::info(
            "This chat was not started by the app (no $CHARTER_HOOK_SOCKET and $CHARTER_CHAT), \
             so its tab will not close by itself — close it when you are done.",
        );
        return;
    };
    let saved = SessionSaved {
        chat,
        session_saved: path.to_path_buf(),
    };
    let token = hookwire::ChatToken::from_env();
    let Err(e) = hookwire::tell_saved(Path::new(&socket), token.as_ref(), &saved) else {
        voice::info("Told the app: a Smart close waiting on this record closes the tab.");
        return;
    };
    let left = relay::Marker {
        chat,
        conversation,
        session_saved: saved.session_saved,
        at: unix_now(),
    };
    match relay::leave(here.plane.root(), &here.place(None), &left) {
        Ok(_) => voice::info(&format!(
            "The app did not hear it ({e}), so this chat's hook tells it when this turn ends. \
             If the tab is still open after that, close it yourself."
        )),
        Err(kept) => voice::warn(&format!(
            "The app did not hear it ({e}), and charter could not leave it for this chat's hook \
             ({kept}), so the tab will not close by itself — close it yourself when you are done."
        )),
    }
}

/// `charter hook stop` in a chat the app started: the saved-record line `charter session
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
        eprintln!("charter: the app did not take this chat's saved session record ({why})");
    }
}

/// Seconds since the Unix epoch, as a marker records when it was left.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
