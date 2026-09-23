//! `charter handoff` — open a chat in a workspace you name, started on an approved brief.
//!
//! ```text
//! charter handoff <workspace> [--create --vision "<vision>"] [--persona <name>] <<'BRIEF'
//! <the brief>
//! BRIEF
//! ```
//!
//! A port of `charter/commands_handoff.py`: the ORDER of the refusals and the writes.
//! [`charter_core::handoff`] is every string they are made of, and its module header is where
//! each refusal is argued — including the four that belong to the PreToolUse guard and are
//! therefore not here.
//!
//! **The order is the design.** Every question is asked before the first write, because
//! charter fails toward no change: a handoff that created the workspace and then discovered
//! the brief was empty has already cost somebody a cleanup.
//!
//! 1. the workspace name, the flag pairs, the persona — questions answered from the plane;
//! 2. the brief on stdin, read before the frame check because the command charter prints when
//!    there is no frame has to carry it;
//! 3. the first message's own shape — empty, a flag, one word, a NUL, past the byte bound;
//! 4. the host: the app that started this chat, or — where there is none — the printed command.
//!
//! # Where this charter opens a chat, and where it prints the command instead
//!
//! Python opens a handed-off chat in a background window of its own tmux frame. This charter
//! has no tmux; its frame is the desktop app, and a chat the app started carries the app's
//! hook socket in `$CHARTER_HOOK_SOCKET` (`charter_core::hookwire`). So step 4 asks the app,
//! over that socket, to open the chat: a tab in the target workspace, started on the stamped
//! brief (charter-app#204). The consent is unchanged and is not asked for twice. It is the
//! harness's permission prompt in front of this exact command, which the operator answered
//! with the brief on screen.
//!
//! **Anything short of the app saying "opened" is today's answer, byte for byte**: no socket
//! in the environment (a terminal, which is where every chat was before #204), an app that is
//! not listening, one that does not answer in time, or one that answers nonsense. That answer
//! prints the command to run in a new terminal and exits 1, which is Python's answer to a
//! shell that is not a chat. An app that answers with a *refusal* gets the same command plus
//! one line saying why, because a refusal nobody sees is a handoff that vanished.
//!
//! What the ticket on that socket is worth, and what it is not, is argued once, where it is
//! implemented: `charter_core::hookwire::OpenChat`.
//!
//! **Python's writes after the open are not ported**: the todo in the target workspace, the
//! dispatch tally, the arrival mark on the strip. This charter has no todo store and writes
//! no dispatch log, and the recorded scenario declares the difference rather than hiding it
//! (`handoff-inside-the-app-opens-the-chat-there-and-writes-nothing`, ADR 0046). With `--create`
//! the app creates the workspace before it opens the chat, because a chat has to stand in a
//! directory that exists.

use std::io::{IsTerminal, Read};
use std::process::ExitCode;

use charter_core::handoff::{self, BadMessage, NoBrief};

use crate::voice;

/// What `charter handoff` was asked to do.
pub struct Args {
    pub workspace: String,
    pub create: bool,
    pub vision: Option<String>,
    pub persona: Option<String>,
}

/// The most personas a refusal lists before it says how many it left out —
/// `frame/switch._SOME`.
const SOME: usize = 5;

pub fn handoff(here: &crate::Here, args: &Args) -> ExitCode {
    let root = here.plane.root();
    let ws = args.workspace.as_str();

    if !charter_core::contain::workspace_name_ok(ws) {
        voice::err(&format!(
            "charter handoff: '{}' cannot name a workspace — nothing was opened. A workspace \
             name is letters, digits, '.', '_' and '-', and does not start with a dot.",
            charter_core::personas::one_line(ws)
        ));
        return ExitCode::FAILURE;
    }
    let vision = args.vision.as_deref().filter(|v| !v.is_empty());
    if vision.is_some() && !args.create {
        voice::err(&format!(
            "charter handoff: --vision describes a workspace this call creates, and '{ws}' is \
             not being created — nothing was opened. Set an existing workspace's vision with: \
             charter workspace vision --workspace {ws} \"<the goal>\""
        ));
        return ExitCode::FAILURE;
    }
    if args.create && vision.is_none() {
        voice::err(
            "charter handoff: --create needs --vision — a workspace with no vision is never \
             proposed as a target, so it would be created unfindable. Nothing was opened.",
        );
        return ExitCode::FAILURE;
    }
    // `workspace use`'s rule, asked rather than re-invented: the always-present workspace
    // exists whether or not its directory does, so `--create default` on a fresh plane is a
    // name collision rather than a creation.
    let exists = here
        .plane
        .workspaces()
        .unwrap_or_default()
        .iter()
        .any(|n| n == ws)
        || ws == charter_core::active::plane_default_workspace(root);
    if args.create && exists {
        voice::err(&format!(
            "charter handoff: workspace '{ws}' already exists, and --create only makes a new \
             one — nothing was opened. Drop --create to hand off into it."
        ));
        return ExitCode::FAILURE;
    }
    if !args.create && !exists {
        voice::err(&format!(
            "charter handoff: no workspace '{ws}' on this plane — nothing was opened. Create \
             it in the same call: charter handoff {ws} --create --vision \"<what it is for>\""
        ));
        return ExitCode::FAILURE;
    }
    // Existence is the DEFINITION's answer, as it is for `persona use`, and not membership in
    // the roster — which also lists a directory whose `persona.md` does not load. The sentence
    // is `persona.name_refusal`'s, the one every command that takes a persona name says.
    let persona = args.persona.as_deref().filter(|p| !p.is_empty());
    if let Some(name) = persona
        && let Some(refused) = charter_core::personas::name_refusal(root, name)
    {
        voice::err(&format!(
            "charter handoff: {refused} — have: {}. Nothing was opened.",
            some(&here.plane.personas().unwrap_or_default())
        ));
        return ExitCode::FAILURE;
    }

    let brief = match read_brief() {
        Ok(brief) => brief,
        Err(refusal) => {
            voice::err(&refusal.say(ws));
            return ExitCode::FAILURE;
        }
    };
    // The leak guard's own classifier, so the brief a handoff refuses and the command a leak
    // guard refuses are the same set of shapes. The KIND, never the matched text — a refusal
    // that quoted the credential would put it in the transcript this exists to keep it out of.
    if let Some(kind) = charter_core::secretshape::secret_kind(&brief) {
        voice::err(&format!(
            "charter handoff: the brief looks like it carries a secret ({kind}) — nothing was \
             opened. A brief travels to the new chat as a command-line argument any local \
             process can read while the harness starts, so it never carries a secret. Name \
             where the credential lives instead of pasting it, as the whole value on its \
             line: `vault:<vault>/<key>`."
        ));
        return ExitCode::FAILURE;
    }

    let source_chat = std::env::var("CHARTER_SESSION_ID")
        .ok()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| handoff::NO_CHAT.to_string());
    let source_ws = here.active_workspace(None);
    let msg = handoff::first_message(
        &handoff::stamp(&source_chat, &source_ws, chrono::Local::now().naive_local()),
        &brief,
    );
    if let Some(bad) = handoff::bad_message(&msg) {
        let mut said = format!("charter handoff: {}", bad.say());
        // Only the byte bound gets the note, and it is compared against the seam's own
        // sentence rather than re-deriving the bound here: two places counting bytes is how
        // the note comes to appear beside a refusal that was about something else.
        if matches!(bad, BadMessage::TooLong(_)) {
            said.push('\n');
            said.push_str(&handoff::stamped_message_note(brief.len()));
        }
        voice::err(&said);
        return ExitCode::FAILURE;
    }

    // ---- the host: the app that started this chat, if one did ---------------------------
    let create_vision = vision.filter(|_| args.create);
    let refused = match in_the_app(ws, &msg, create_vision, persona) {
        Host::Opened(chat) => {
            // `commands_handoff.OPENED`, word for word, on stdout where Python prints it.
            println!(
                "charter handoff: opened chat {chat} in workspace '{ws}', started on the brief"
            );
            return ExitCode::SUCCESS;
        }
        Host::Refused(why) => Some(why),
        Host::None => None,
    };

    // ---- no app that would open it --------------------------------------------------
    // The app is the only thing that opens a chat: this binary has no window of its own and no
    // command that starts a harness. So the answer is where to go, not a command to paste.
    let mut said = match refused {
        // The app answered and said no: that is the whole answer, in its words.
        Some(why) => format!(
            "charter handoff: the charter app that started this chat was asked, and would not \
             open one: {} — nothing was opened.",
            charter_core::personas::one_line(&why)
        ),
        None => format!(
            "charter handoff: no charter app answered this call, so nothing was opened. Open \
             charter, then run this handoff again from a chat the app started — or start a \
             chat in workspace '{ws}' from the window and give it the brief."
        ),
    };
    if create_vision.is_some() {
        said.push('\n');
        said.push_str(&format!(
            "  '{ws}' was not created either: --create makes it when the app opens the chat."
        ));
    }
    voice::err(&said);
    ExitCode::FAILURE
}

/// What the app that started this chat did with a handoff.
enum Host {
    /// The chat is open, under this number on the app's board.
    Opened(u32),
    /// The app answered, and said no, in its own words.
    Refused(String),
    /// There is no app to ask, or it did not answer: the terminal path, unchanged.
    None,
}

/// How long the app has to mint a ticket. It is a map insert; two seconds is an app that is
/// not going to answer.
const A_TICKET_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(2);

/// How long the app has to open the chat. It resolves a profile, which can run a subprocess
/// to ask whether a file is tracked, and spawns a harness, so this is generous. It is still a
/// bound, because the operator is waiting on a Bash tool call that has to end.
const AN_OPEN_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(20);

/// Asks the app that started this chat to open the handoff (charter-app#204).
///
/// Two lines on ONE connection: a ticket for this chat, then the open that spends it. See
/// `charter_core::hookwire::OpenChat` for what that ticket guarantees and what it does not.
///
/// **No app is not an error here, it is the terminal.** `$CHARTER_HOOK_SOCKET` and
/// `$CHARTER_CHAT` are set only in a chat the app started (`sessions.rs`), so their absence is
/// every chat charter had before the app existed, and it gets the answer it always got. Every
/// failure after that (nothing listening, a deadline passed, a line that will not parse) is
/// treated the same way, silently. `hookwire`'s own rule is that it can never break a turn,
/// and the printed command is a handoff the operator can still carry out.
fn in_the_app(ws: &str, msg: &str, create_vision: Option<&str>, persona: Option<&str>) -> Host {
    use charter_core::hookwire::{Answer, Ask, Asking, CHAT_ENV, OpenChat, SOCKET_ENV};

    let Some(socket) = std::env::var_os(SOCKET_ENV).filter(|s| !s.is_empty()) else {
        return Host::None;
    };
    let Some(chat) = std::env::var(CHAT_ENV)
        .ok()
        .and_then(|chat| chat.parse::<u32>().ok())
    else {
        return Host::None;
    };
    let Ok(mut asking) = Asking::on(std::path::Path::new(&socket)) else {
        return Host::None;
    };
    let ticket = match asking.ask(&Ask::Ticket { chat }, A_TICKET_TAKES_AT_MOST) {
        Ok(Answer::Ticket { ticket }) => ticket,
        Ok(Answer::No { why }) => return Host::Refused(why),
        Ok(Answer::Opened { .. }) | Err(_) => return Host::None,
    };
    let open = OpenChat {
        chat,
        workspace: ws.to_owned(),
        create_vision: create_vision.map(str::to_owned),
        persona: persona.map(str::to_owned),
        message: msg.to_owned(),
        ticket,
    };
    match asking.ask(&Ask::Open(Box::new(open)), AN_OPEN_TAKES_AT_MOST) {
        Ok(Answer::Opened { chat }) => Host::Opened(chat),
        Ok(Answer::No { why }) => Host::Refused(why),
        Ok(Answer::Ticket { .. }) | Err(_) => Host::None,
    }
}

/// The brief on stdin, or why there is none.
///
/// The bytes are read as BYTES and decoded here, because the decision "is this UTF-8" has to
/// be charter's: a runtime that decoded for us would have made it already, with whatever
/// replacement policy it has.
///
/// `None` for a stdin that is not there at all — `charter handoff beta 0<&-`, a spelling the
/// Bash guard allows because the two words in front of it are the exact ones. Python meets it
/// as `sys.stdin is None`; here fd 0 is closed, so the read fails with `EBADF`.
fn read_brief() -> Result<String, NoBrief> {
    let stdin = std::io::stdin();
    // Before any read, and that order is the whole function: a read on a terminal blocks for
    // ever with nothing on screen to say why.
    if stdin.is_terminal() {
        return Err(NoBrief::Terminal);
    }
    let mut bytes = Vec::new();
    match stdin.lock().read_to_end(&mut bytes) {
        Ok(_) => charter_core::handoff::read_brief(Some(&bytes), false),
        Err(e) if e.raw_os_error() == Some(9) => charter_core::handoff::read_brief(None, false),
        // Any other read failure is not a brief either, and "not UTF-8" is the wrong
        // sentence for it — but an I/O error on a pipe charter was handed is a brief that is
        // not there, which is what `Closed` says.
        Err(_) => charter_core::handoff::read_brief(None, false),
    }
}

/// `names`, contained, capped, saying how many were left out — `frame/switch._some`.
fn some(names: &[String]) -> String {
    if names.is_empty() {
        return "none".to_string();
    }
    let mut shown: Vec<String> = names
        .iter()
        .take(SOME)
        .map(|n| charter_core::personas::one_line(n))
        .collect();
    if names.len() > shown.len() {
        shown.push(format!("…{} more", names.len() - shown.len()));
    }
    shown.join(", ")
}
