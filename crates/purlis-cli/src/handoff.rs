//! `charter handoff` — open a chat in a workspace you name, started on a brief.
//!
//! ```text
//! charter handoff --name "<task>" <workspace> [--create --vision "<vision>"]
//!     [--persona <name>] <<'BRIEF'
//! <the brief>
//! BRIEF
//! ```
//!
//! A port of `charter/commands_handoff.py`: the ORDER of the refusals and the writes.
//! [`purlis_core::handoff`] is every string they are made of, and its module header is where
//! each refusal is argued — including the ones that belong to the PreToolUse guard and are
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
//! **A handoff is fire-and-forget** (#1515): the person's work moves to a chat they will read
//! themselves. Work the asking chat needs an answer from is a task, and its one route is
//! `purlis dispatch`. `--report` is still taken, because chats have learned it, and is
//! carried out as that task ([`as_a_task`]): the ask a dispatch sends, into the workspace
//! named, with a line saying which route to use from now on.
//!
//! # Where this charter opens a chat, and where it prints the command instead
//!
//! Python opens a handed-off chat in a background window of its own tmux frame. This charter
//! has no tmux; its frame is the desktop app, and a chat the app started carries the app's
//! hook socket in `$CHARTER_HOOK_SOCKET` (`purlis_core::hookwire`). So step 4 asks the app,
//! over that socket, to open the chat: a tab in the target workspace, started on the stamped
//! brief (charter-app#204).
//!
//! **The app decides it, as it decides any dispatch** (#1444): a handoff is a dispatch in
//! handoff mode, and its consent is the dispatch grant, never the harness's permission
//! prompt. To the asking chat's own persona it opens at once. To another, the app holds it
//! and asks the person for the pair on this chat's tab, with the brief in front of them;
//! this command says so and exits 0, because a held handoff was accepted, not refused
//! ([`held_for_the_person`]). A second one across a pair the person is already being asked
//! about is dropped, and that is a refusal ([`not_held`]). Nobody approves the brief.
//!
//! **Anything short of the app saying "opened" is today's answer, byte for byte**: no socket
//! in the environment (a terminal, which is where every chat was before #204), an app that is
//! not listening, one that does not answer in time, or one that answers nonsense. That answer
//! prints the command to run in a new terminal and exits 1, which is Python's answer to a
//! shell that is not a chat. An app that answers with a *refusal* gets the same command plus
//! one line saying why, because a refusal nobody sees is a handoff that vanished.
//!
//! What the ticket on that socket is worth, and what it is not, is argued once, where it is
//! implemented: `purlis_core::hookwire::OpenChat`.
//!
//! **After the open, the command writes what Python wrote around its own** (#372): the todo in
//! the target workspace, de-duplicated by first line, and one `handoff` row in the dispatch
//! log ([`record_opened`]). Both are best-effort and never undo the open. The arrival mark on
//! the strip is the app's: it draws the new tab without taking the screen. The recorded
//! scenario is `handoff-inside-the-app-opens-the-chat-there-and-records-its-todo` (ADR 0046).
//! With `--create` the app creates the workspace before it opens the chat, because a chat has
//! to stand in a directory that exists.

use std::io::{IsTerminal, Read};
use std::process::ExitCode;

use purlis_core::handoff::{self, BadMessage, NoBrief};
use purlis_core::names::HANDOFF_SAYS;

use crate::voice;

/// What `charter handoff` was asked to do.
pub struct Args {
    pub workspace: String,
    pub create: bool,
    pub vision: Option<String>,
    pub persona: Option<String>,
    /// `--name`: what the new chat is called (charter-app#258).
    pub name: Option<String>,
    /// `--report`: this chat needs an answer, so the work is dispatched as a task (#1515).
    pub report: bool,
    /// The word after `report` in `charter handoff report "<summary>"`.
    pub summary: Option<String>,
    /// The hidden `--now`: a local naive time the stamp, the todo and the dispatch row are
    /// written at, so a recorded scenario can pin them.
    pub now: Option<String>,
}

impl Args {
    /// Whether what this asks for, once it succeeds, is a handoff an extension is told of
    /// (`handoff-created`).
    ///
    /// **Not with `--report`** (#1515): that is carried out as a task, a dispatch tells
    /// extensions nothing, and a task that told them a handoff was created would be the one
    /// respect in which it is not a task. There is no event for a dispatch to tell in its
    /// place, so nothing is told.
    pub fn creates_a_handoff(&self) -> bool {
        !self.report
    }
}

/// The word that made `purlis handoff` a report back rather than a handoff, and is now refused
/// naming `purlis dispatch report` (#1471).
///
/// **Only with a summary after it.** `purlis handoff report <<'BRIEF'` is still a handoff
/// into a workspace called `report`, as it always was.
pub const REPORT: &str = "report";

/// The most personas a refusal lists before it says how many it left out —
/// `frame/switch._SOME`.
const SOME: usize = 5;

pub fn handoff(here: &crate::Here, args: &Args) -> ExitCode {
    let root = here.plane.root();
    let ws = args.workspace.as_str();

    if args.summary.is_some() {
        if ws == REPORT {
            return report_back();
        }
        voice::err(&format!(
            "{HANDOFF_SAYS} takes one workspace and its brief on stdin, and was given a \
             second word after '{}' — nothing was opened. A report is sent with purlis \
             dispatch report --outcome done \"<summary>\"",
            purlis_core::personas::one_line(ws)
        ));
        return ExitCode::FAILURE;
    }

    if !purlis_core::contain::workspace_name_ok(ws) {
        voice::err(&format!(
            "{HANDOFF_SAYS} '{}' cannot name a workspace — nothing was opened. A workspace \
             name is letters, digits, '.', '_' and '-', and does not start with a dot.",
            purlis_core::personas::one_line(ws)
        ));
        return ExitCode::FAILURE;
    }
    let vision = args.vision.as_deref().filter(|v| !v.is_empty());
    if vision.is_some() && !args.create {
        voice::err(&format!(
            "{HANDOFF_SAYS} --vision describes a workspace this call creates, and '{ws}' is \
             not being created — nothing was opened. Set an existing workspace's vision with: \
             purlis workspace vision --workspace {ws} \"<the goal>\""
        ));
        return ExitCode::FAILURE;
    }
    if args.create && vision.is_none() {
        voice::err(&format!(
            "{HANDOFF_SAYS} --create needs --vision — a workspace with no vision is never \
             proposed as a target, so it would be created unfindable. Nothing was opened."
        ));
        return ExitCode::FAILURE;
    }
    // A handoff that reports back is a task (#1515), and a task works in a workspace the
    // project already has: `purlis dispatch --in workspace:<name>` makes none. Said before
    // anything is read or asked, with the two commands that do it.
    if args.report && args.create {
        voice::err(&format!(
            "{HANDOFF_SAYS} --report makes this a task, and a task works in a workspace that \
             exists, so --create cannot go with it — nothing was opened. {} {}",
            create_then_dispatch(ws),
            handoff::ONE_ROUTE
        ));
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
        || ws == purlis_core::active::plane_default_workspace(root);
    if args.create && exists {
        voice::err(&format!(
            "{HANDOFF_SAYS} workspace '{ws}' already exists, and --create only makes a new \
             one — nothing was opened. Drop --create to hand off into it."
        ));
        return ExitCode::FAILURE;
    }
    // With `--report` the way forward is not `--create`, which a task refuses above.
    if !args.create && !exists && args.report {
        voice::err(&format!(
            "{HANDOFF_SAYS} no workspace '{ws}' on this plane — nothing was opened. --report \
             makes this a task, and a task works in a workspace that exists. {}",
            create_then_dispatch(ws)
        ));
        return ExitCode::FAILURE;
    }
    if !args.create && !exists {
        voice::err(&format!(
            "{HANDOFF_SAYS} no workspace '{ws}' on this plane — nothing was opened. Create \
             it in the same call: charter handoff {ws} --create --vision \"<what it is for>\""
        ));
        return ExitCode::FAILURE;
    }
    // Existence is the DEFINITION's answer, as it is for `persona use`, and not membership in
    // the roster — which also lists a directory whose `persona.md` does not load. The sentence
    // is `persona.name_refusal`'s, the one every command that takes a persona name says.
    let persona = args.persona.as_deref().filter(|p| !p.is_empty());
    if let Some(name) = persona
        && let Some(refused) = purlis_core::personas::name_refusal(root, name)
    {
        voice::err(&format!(
            "{HANDOFF_SAYS} {refused} — have: {}. Nothing was opened.",
            some(&here.plane.personas().unwrap_or_default())
        ));
        return ExitCode::FAILURE;
    }

    // The name is a question answered without the brief, so it is asked before the read, and
    // by the rule every name a chat has is held to.
    let name = match args.name.as_deref().map(purlis_core::reopen::label) {
        None => None,
        Some(Ok(name)) => name,
        Some(Err(why)) => {
            voice::err(&format!("{HANDOFF_SAYS} --name: {why} Nothing was opened."));
            return ExitCode::FAILURE;
        }
    };

    let brief = match read_brief() {
        Ok(brief) => brief,
        Err(refusal) => {
            voice::err(&refusal.say(ws));
            return ExitCode::FAILURE;
        }
    };
    // The leak guard's own classifier, so the brief a handoff refuses and the command a leak
    // guard refuses are the same set of shapes — as written and through its escapes (#1315).
    // The KIND, never the matched text — a refusal that quoted the credential would put it in
    // the transcript this exists to keep it out of.
    if let Some(kind) = purlis_core::secretshape::kind_as_read(None, &brief) {
        voice::err(&format!(
            "{HANDOFF_SAYS} the brief looks like it carries a secret ({kind}) — nothing was \
             opened. A brief travels to the new chat as a command-line argument any local \
             process can read while the harness starts, so it never carries a secret. Name \
             where the credential lives instead of pasting it, as the whole value on its \
             line: `vault:<vault>/<key>`."
        ));
        return ExitCode::FAILURE;
    }

    if args.report {
        return as_a_task(here, ws, persona, name.as_deref(), &brief);
    }

    let source_chat = purlis_core::envvar::var("PURLIS_SESSION_ID")
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| handoff::NO_CHAT.to_string());
    // Where this chat works, which is what the stamp and the todo say it left from: a chat
    // at the plane root is in no workspace, and its stamp says the plane root rather than the
    // workspace the ladder would have picked for it (SI-1b).
    let source = here.place(None);
    let now = match when(args.now.as_deref()) {
        Ok(now) => now,
        Err(why) => {
            voice::err(&format!("{HANDOFF_SAYS} {why}"));
            return ExitCode::FAILURE;
        }
    };
    let msg = handoff::first_message(
        &handoff::stamp(&source_chat, &source, now.naive_local()),
        &brief,
    );
    // Measured as the chat will be sent it: with the stamp naming the chat that asks — which
    // the app writes, by the name it shows. A name longer than this one's number can still
    // take a message over the bound, and the app says so in that case; this is the refusal
    // nearly every such brief gets. No report line: a handoff that asks for one went as a task.
    let sent = handoff::delivered(&msg, &source_chat).unwrap_or_else(|| msg.clone());
    if let Some(bad) = handoff::bad_message(&sent) {
        let mut said = format!("{HANDOFF_SAYS} {}", bad.say());
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
    let refused = match in_the_app(ws, &msg, create_vision, persona, name) {
        Host::Opened(chat, row, note) => {
            record_opened(&Opened {
                here,
                chat,
                row,
                ws,
                brief: &brief,
                source_chat: &source_chat,
                source: &source,
                created: args.create,
                now,
            });
            // `commands_handoff.OPENED`, word for word, on stdout where Python prints it.
            println!("{HANDOFF_SAYS} opened chat {chat} in workspace '{ws}', started on the brief");
            // What the app said about the profile it started on (#1445, D-1445-8): the new chat
            // is not on its persona's own. One line, contained: it names a value out of a file.
            if let Some(note) = note {
                voice::info(&whole(&note));
            }
            return ExitCode::SUCCESS;
        }
        // Held, not refused: the handoff is accepted and waits on the person, so this is
        // not a failure and is not said as one. Nothing was opened, so nothing is recorded:
        // the app opens the chat when the person allows the pair.
        Host::Held {
            from,
            to,
            waiting: None,
        } => {
            println!(
                "{}",
                held_for_the_person(from.as_deref(), &to, ws, create_vision.is_some())
            );
            return ExitCode::SUCCESS;
        }
        // Not held: the person is already being asked about this pair for another dispatch,
        // and this one was dropped. Nothing of it will open, so it is a refusal.
        Host::Held {
            from,
            to,
            waiting: Some(first),
        } => {
            voice::err(&not_held(from.as_deref(), &to, &first));
            return ExitCode::FAILURE;
        }
        // Held, not refused: it opens by itself once memory frees (#1467).
        Host::WaitingOnMemory { to } => {
            println!(
                "{}",
                waiting_on_memory(to.as_deref(), ws, create_vision.is_some())
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
            "{HANDOFF_SAYS} the purlis app that started this chat was asked, and would not \
             open one: {} — nothing was opened.",
            whole(&why)
        ),
        None => format!(
            "{HANDOFF_SAYS} no purlis app answered this call, so nothing was opened. Open \
             purlis, then run this handoff again from a chat the app started — or start a \
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

/// The two commands that do what `--report` cannot do in one, said by both refusals about a
/// workspace that is not there: create it **with its vision** (a workspace with none is never
/// proposed, which is why `--create` needs `--vision`), then dispatch into it.
fn create_then_dispatch(ws: &str) -> String {
    format!(
        "Create '{ws}' first (purlis workspace create {ws} --vision \"<what it is for>\"), \
         then dispatch into it: purlis dispatch --name \"<task>\" --in workspace:{ws}."
    )
}

/// **A handoff that asks for a report is a task, and is dispatched as one** (#1515).
///
/// A handoff is the person's work moving to a chat they will read themselves. Work the asking
/// chat needs an answer from is a task: listed under it, waited on, steered and cancelled.
/// `--report` used to be a second route to the second thing, with none of a task's handles: its
/// report waited for the asking chat's next turn and nothing started that turn, `purlis
/// dispatch wait` refused its chat and `list` did not show it.
///
/// So this sends **the ask `purlis dispatch` sends** ([`crate::dispatch::send`]), with the
/// workspace the handoff named as the place the task works (`--in workspace:<name>`), and the
/// app cannot tell the two apart: it is a task in every respect because it is one. What the
/// command adds is the line saying so, and the route from now on
/// ([`handoff::reported_as_a_task`]), after a start, a hold and a refusal alike.
///
/// A task leaves no todo in the workspace it works in and no `handoff` row in the dispatch
/// log: its dispatch record is the app's.
fn as_a_task(
    here: &crate::Here,
    ws: &str,
    persona: Option<&str>,
    name: Option<&str>,
    brief: &str,
) -> ExitCode {
    let task = name.map_or_else(|| handoff::task_name_of_a_handoff(ws), str::to_owned);
    let place = format!("{}{ws}", purlis_core::dispatchplace::WORKSPACE);
    let options = crate::dispatch::Options {
        profile: None,
        place: Some(&place),
    };
    match crate::dispatch::send(here, persona, &task, brief, &options, None) {
        Ok(said) => {
            println!("{said}");
            println!("{}", handoff::reported_as_a_task());
            ExitCode::SUCCESS
        }
        Err(why) => {
            voice::err(&why);
            voice::info(&handoff::reported_as_a_task());
            ExitCode::FAILURE
        }
    }
}

/// Who asks, as a held handoff's sentences say it.
fn who_asks(from: Option<&str>) -> String {
    match from {
        Some(from) => format!("'{}' chats", purlis_core::personas::one_line(from)),
        None => "this chat".to_owned(),
    }
}

/// What a chat is told where its handoff waits on the person for a dispatch grant: nothing
/// has opened, the person is being asked on this chat's tab, and what happens next. A
/// dispatch's own sentence (`crate::dispatch`), in a handoff's words.
fn held_for_the_person(from: Option<&str>, to: &str, ws: &str, creates: bool) -> String {
    let created = if creates {
        format!(" '{ws}' is created then too, and not before.")
    } else {
        String::new()
    };
    format!(
        "{HANDOFF_SAYS} held for the person. {} may not dispatch to '{}' yet, so the person \
         is being asked on this chat's tab, with this brief in front of them. Nothing has been \
         opened. If they allow it, the chat opens in workspace '{ws}' then, started on the \
         brief;{created} if they keep it blocked, this chat is told on its next turn. Carry on \
         with other work, and do not hand it off again.",
        who_asks(from),
        purlis_core::personas::one_line(to)
    )
}

/// What a chat is told where its handoff waits on this machine's memory (#1467).
fn waiting_on_memory(to: Option<&str>, ws: &str, creates: bool) -> String {
    let to = to.map_or_else(String::new, |to| {
        format!(" as '{}'", purlis_core::personas::one_line(to))
    });
    let created = if creates {
        format!(" '{ws}' is created then too, and not before.")
    } else {
        String::new()
    };
    format!(
        "{HANDOFF_SAYS} waiting on memory. This machine is short on memory, so nothing has been \
         opened yet. The chat{to} opens by itself in workspace '{ws}' once memory frees, \
         started on the brief and held to the limits as they are then.{created} If memory is \
         still short after {} minutes, nothing opens and this chat is told on its next turn. \
         Carry on with other work, and do not hand it off again.",
        purlis_core::dispatchdecision::MEMORY_WAIT_MINUTES
    )
}

/// What a chat is told where the person is already being asked about this pair for the
/// dispatch `first`: this handoff was not held beside it, because the person was shown one
/// brief.
fn not_held(from: Option<&str>, to: &str, first: &str) -> String {
    let one = purlis_core::personas::one_line;
    format!(
        "{HANDOFF_SAYS} not held. The person is already being asked whether {} may dispatch \
         to '{}', for '{}', and they were shown that brief, so only it starts when they allow \
         it. Nothing was opened. Hand this off again once they have answered.",
        who_asks(from),
        one(to),
        one(first)
    )
}

/// The instant this handoff happens at: the hidden `--now`, a LOCAL naive time as it is
/// everywhere in this binary, else the clock. A `--now` that names no single local instant
/// is refused rather than swapped for the clock.
fn when(now: Option<&str>) -> Result<chrono::DateTime<chrono::Local>, String> {
    let Some(text) = now else {
        return Ok(chrono::Local::now());
    };
    let naive: chrono::NaiveDateTime = text
        .parse()
        .map_err(|e| format!("--now is not a local naive timestamp: {e}"))?;
    chrono::TimeZone::from_local_datetime(&chrono::Local, &naive)
        .single()
        .ok_or_else(|| "--now names no single local instant".to_owned())
}

/// What an opened handoff leaves behind (#372): the todo in the target workspace, and one
/// row in the dispatch log — `commands_handoff.py`'s writes around its open.
///
/// **After the open, and never able to undo it.** Python records the todo before it opens;
/// here the app is what opens, and with `--create` it is also what makes the workspace, so
/// the todo cannot be written any earlier. A handoff the app would not open leaves nothing.
/// Each write is best-effort: one that fails is said, on stderr as a `!`, because the chat
/// is open and the command did what it was asked.
fn record_opened(opened: &Opened<'_>) {
    let (chat, ws) = (opened.chat, opened.ws);
    match record_todo(opened) {
        Ok(Todo::Recorded) => {}
        // Reported and continued: a second chat on the same work may be exactly what was
        // approved, and charter makes no judgement about the content of work.
        Ok(Todo::AlreadyListed(dup)) => voice::info(&format!(
            "already on '{ws}'s list: {} — not recorded twice",
            purlis_core::personas::one_line(&dup)
        )),
        Err(why) => voice::warn(&format!(
            "{HANDOFF_SAYS} chat {chat} is open in '{ws}', but its todo could not be \
             recorded there ({}). Record it with: purlis workspace todo -w {ws} \"<the \
             brief's first line>\"",
            purlis_core::personas::one_line(&why)
        )),
    }
    // The app writes the row where it opened the chat (#1421): a sandboxed chat may not write
    // the project's `personas/_dispatch/`. An app that leaves it here gets it written here.
    let unwritten = match &opened.row {
        Some(purlis_core::hookwire::Row::Written) => None,
        Some(purlis_core::hookwire::Row::Unwritten { why }) => Some(why.clone()),
        None => {
            let placement = if opened.source.workspace() == Some(ws) {
                purlis_core::dispatch::Placement::Here
            } else {
                purlis_core::dispatch::Placement::Elsewhere
            };
            purlis_core::dispatch::record_handoff(
                opened.here.plane.root(),
                placement,
                opened.created,
                opened.now.with_timezone(&chrono::Utc),
                &purlis_core::machine::this_log_name(),
            )
            .err()
            .map(|why| purlis_core::rewrite::os_words(&why))
        }
    };
    if let Some(why) = unwritten {
        // The chat is open and its todo recorded: only the count of handoffs misses one. The
        // OS's words, not the rewording, which would name the log's path a second time and
        // ask for a rerun that would open a second chat (#1359).
        voice::warn(&format!(
            "{HANDOFF_SAYS} chat {chat} is open in '{ws}'. Only its row in the dispatch log \
             (personas/{}) is missing ({}); there is nothing to run again.",
            purlis_core::dispatch::DIR_NAME,
            purlis_core::personas::one_line(&why)
        ));
    }
}

/// What became of an opened handoff's todo.
enum Todo {
    Recorded,
    /// An open todo there is already about the same work; this is its title.
    AlreadyListed(String),
}

fn record_todo(opened: &Opened<'_>) -> Result<Todo, String> {
    let text = handoff::todo_text(opened.brief, opened.source_chat, opened.source);
    let target = opened
        .here
        .plane
        .workspace(opened.ws)
        .map_err(|e| e.to_string())?;
    if let Some(dup) = target.todo_for_the_same_work(&text) {
        return Ok(Todo::AlreadyListed(dup));
    }
    target
        .add_todo(&text, opened.now.naive_local())
        .map(|_| Todo::Recorded)
        .map_err(|e| e.to_string())
}

/// A handoff the app opened, and the facts [`record_opened`] writes down about it.
struct Opened<'a> {
    here: &'a crate::Here,
    chat: u32,
    ws: &'a str,
    brief: &'a str,
    source_chat: &'a str,
    /// Where the chat that handed off works: a workspace, or the plane root.
    source: &'a purlis_core::active::Place,
    created: bool,
    now: chrono::DateTime<chrono::Local>,
    /// The row the app wrote, or why it could not; `None` from an app that leaves it to this
    /// command.
    row: Option<purlis_core::hookwire::Row>,
}

/// What the app that started this chat did with a handoff.
enum Host {
    /// The chat is open, under this number on the app's board, and what became of its row in
    /// the dispatch log where the app wrote it ([`purlis_core::hookwire::Row`]).
    Opened(u32, Option<purlis_core::hookwire::Row>, Option<String>),
    /// The app holds the handoff and is asking the person for a dispatch grant across this
    /// pair of personas ([`purlis_core::hookwire::Answer::NeedsGrant`]): nothing has opened.
    Held {
        from: Option<String>,
        to: String,
        waiting: Option<String>,
    },
    /// The app holds the handoff while this machine is short on memory (#1467): every check
    /// let it through, and nothing has opened yet.
    WaitingOnMemory { to: Option<String> },
    /// The app answered, and said no, in its own words.
    Refused(String),
    /// There is no app to ask, or it did not answer: the terminal path, unchanged.
    None,
}

/// How long the app has to mint a ticket. It is a map insert; two seconds is an app that is
/// not going to answer.
pub(crate) const A_TICKET_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(2);

/// How long the app has to open the chat. It resolves a profile, which can run a subprocess
/// to ask whether a file is tracked, and spawns a harness, so this is generous. It is still a
/// bound, because the operator is waiting on a Bash tool call that has to end.
pub(crate) const AN_OPEN_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(20);

/// Asks the app that started this chat to open the handoff (charter-app#204).
///
/// Two lines on ONE connection: a ticket for this chat, then the open that spends it. See
/// `purlis_core::hookwire::OpenChat` for what that ticket guarantees and what it does not.
///
/// **No app is not an error here, it is the terminal.** `$CHARTER_HOOK_SOCKET` and
/// `$CHARTER_CHAT` are set only in a chat the app started (`sessions.rs`), so their absence is
/// every chat charter had before the app existed, and it gets the answer it always got. Every
/// failure after that (nothing listening, a deadline passed, a line that will not parse) is
/// treated the same way, silently. `hookwire`'s own rule is that it can never break a turn,
/// and the printed command is a handoff the operator can still carry out.
fn in_the_app(
    ws: &str,
    msg: &str,
    create_vision: Option<&str>,
    persona: Option<&str>,
    name: Option<String>,
) -> Host {
    use purlis_core::hookwire::{Answer, Ask, OpenChat};

    let (mut asking, chat, ticket) = match ticketed() {
        Ticketed::Yes(asking, chat, ticket) => (asking, chat, ticket),
        Ticketed::Refused(why) => return Host::Refused(why),
        Ticketed::NoApp => return Host::None,
    };
    let open = OpenChat {
        chat,
        workspace: ws.to_owned(),
        create_vision: create_vision.map(str::to_owned),
        persona: persona.map(str::to_owned),
        message: msg.to_owned(),
        ticket,
        name,
        // Never asked for, and never written (#1515, #1471): a handoff that wants an answer is
        // sent as a task ([`as_a_task`]), so what this opens owes no report.
        older_report: false,
    };
    match asking.ask(&Ask::Open(Box::new(open)), AN_OPEN_TAKES_AT_MOST) {
        Ok(Answer::Opened { chat, row, note }) => Host::Opened(chat, row, note),
        Ok(Answer::NeedsGrant { from, to, waiting }) => Host::Held { from, to, waiting },
        Ok(Answer::WaitingOnMemory { to }) => Host::WaitingOnMemory { to },
        Ok(Answer::No { why }) => Host::Refused(why),
        Ok(
            Answer::Ticket { .. }
            | Answer::Reported { .. }
            | Answer::Finished { .. }
            | Answer::Recorded { .. }
            | Answer::Written { .. }
            | Answer::Said { .. }
            | Answer::Vaults { .. }
            | Answer::Working(_)
            | Answer::Dispatched { .. }
            | Answer::Task(_),
        )
        | Err(_) => Host::None,
    }
}

/// A ticket from the app that started this chat, on the connection it must be spent on.
pub(crate) enum Ticketed {
    Yes(purlis_core::hookwire::Asking, u32, String),
    /// The app answered, and would not mint one.
    Refused(String),
    /// No app to ask, or it did not answer.
    NoApp,
}

/// The first of the two lines every ask on the socket is (charter-app#204): a ticket for the
/// chat this process runs in, which `$CHARTER_CHAT` names.
pub(crate) fn ticketed() -> Ticketed {
    use purlis_core::hookwire::{Answer, Ask, Asking, CHAT_ENV, ChatToken, SOCKET_ENV};

    let Some(socket) = purlis_core::envvar::var_os(SOCKET_ENV).filter(|s| !s.is_empty()) else {
        return Ticketed::NoApp;
    };
    let Some(chat) = purlis_core::envvar::var(CHAT_ENV).and_then(|chat| chat.parse::<u32>().ok())
    else {
        return Ticketed::NoApp;
    };
    let Ok(mut asking) = Asking::on(std::path::Path::new(&socket), ChatToken::from_env()) else {
        return Ticketed::NoApp;
    };
    match asking.ask(&Ask::Ticket { chat }, A_TICKET_TAKES_AT_MOST) {
        Ok(Answer::Ticket { ticket }) => Ticketed::Yes(asking, chat, ticket),
        Ok(Answer::No { why }) => Ticketed::Refused(why),
        Ok(
            Answer::Opened { .. }
            | Answer::Reported { .. }
            | Answer::Finished { .. }
            | Answer::Recorded { .. }
            | Answer::Written { .. }
            | Answer::Said { .. }
            | Answer::Vaults { .. }
            | Answer::Working(_)
            | Answer::Dispatched { .. }
            | Answer::NeedsGrant { .. }
            | Answer::WaitingOnMemory { .. }
            | Answer::Task(_),
        )
        | Err(_) => Ticketed::NoApp,
    }
}

/// `purlis handoff report "<summary>"`: **retired, and kept as a refusal** (#1471). Every report
/// is sent by `purlis dispatch report`, with how the work ended: a handoff owes none, and a
/// chat an older purlis handed work owing one is read as the task it was (#1519). The word stays
/// because chats learned it, and this tells them the command that sends a report. Nothing is
/// sent, and the app is not asked.
fn report_back() -> ExitCode {
    voice::err(REPORT_RETIRED);
    ExitCode::FAILURE
}

/// What `purlis handoff report` answers since #1471.
const REPORT_RETIRED: &str = "purlis handoff report: a report is sent with `purlis dispatch \
     report --outcome done \"<what you did and found>\"` (or --outcome blocked or failed), and a \
     handoff owes none — nothing was sent.";

/// What the app said, as one line and **whole**: escaped as anything a chat may have had a
/// hand in is, and never cut. The app's refusals end with what to do, and the budget a name
/// is drawn within (160 characters) would cut most of them off before it.
fn whole(said: &str) -> String {
    purlis_core::shown::one_line(said, purlis_core::shown::NO_CLIP)
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
        Ok(_) => purlis_core::handoff::read_brief(Some(&bytes), false),
        Err(e) if e.raw_os_error() == Some(9) => purlis_core::handoff::read_brief(None, false),
        // Any other read failure is not a brief either, and "not UTF-8" is the wrong
        // sentence for it — but an I/O error on a pipe charter was handed is a brief that is
        // not there, which is what `Closed` says.
        Err(_) => purlis_core::handoff::read_brief(None, false),
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
        .map(|n| purlis_core::personas::one_line(n))
        .collect();
    if names.len() > shown.len() {
        shown.push(format!("…{} more", names.len() - shown.len()));
    }
    shown.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(report: bool) -> Args {
        Args {
            workspace: "alpha".to_owned(),
            create: false,
            vision: None,
            persona: None,
            name: None,
            report,
            summary: None,
            now: None,
        }
    }

    /// #1515: a handoff that asks for a report is a task, and a task tells extensions nothing.
    #[test]
    fn extensions_are_told_of_a_handoff_and_not_of_one_carried_out_as_a_task() {
        assert!(asked(false).creates_a_handoff());
        assert!(!asked(true).creates_a_handoff());
    }

    /// The two commands a refusal about a missing workspace names keep the vision, without
    /// which a workspace is never proposed.
    #[test]
    fn the_way_to_a_workspace_that_is_not_there_keeps_its_vision() {
        assert_eq!(
            create_then_dispatch("gamma"),
            "Create 'gamma' first (purlis workspace create gamma --vision \"<what it is \
             for>\"), then dispatch into it: purlis dispatch --name \"<task>\" --in \
             workspace:gamma."
        );
    }
}
