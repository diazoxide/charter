//! `purlis dispatch` — start a persona chat on a task, and send a task's report back (#1436).
//!
//! ```text
//! purlis dispatch --name "<task>" [--to <persona>] [--profile <profile>] <<'BRIEF'
//! <the brief>
//! BRIEF
//!
//! purlis dispatch report --outcome done|blocked|failed [--changed "<what changed>"] "<text>"
//!
//! purlis dispatch --name "<task>" --wait [--timeout <seconds>] <<'BRIEF' …
//! purlis dispatch wait <chat> [--timeout <seconds>]
//! purlis dispatch list
//! purlis dispatch cancel <chat>
//! ```
//!
//! A dispatch is one chat starting another, which runs as a persona for its whole life
//! (ADR 0090, #1434). This command is the asking chat's half of a **task**: it hands the app the
//! persona, the task's name and the brief, and the app decides
//! ([`purlis_core::dispatchdecision`]) and starts the chat. The `dispatch` chat tool runs the
//! same two functions ([`send`], [`report`]), so the command and the tool cannot answer one
//! request two ways.
//!
//! **It rides the handoff's road**: a ticket for this chat, then the ask that spends it, on one
//! connection of the chat's hook socket ([`crate::handoff::ticketed`]); the stamp the new chat
//! opens with; and the report, which is the handoff's own report ask with a task's outcome on
//! it. What differs is who writes the facts. A handoff stamps its own message and the app
//! checks it; here the request says nothing about the chat that asks but its number, and the
//! app writes the stamp from its own record ([`purlis_core::hookwire::DispatchAsk`]).
//!
//! **Asking after a task** (#1441): `--wait` and `wait` hold the command until the task's report
//! lands, and print the report as the next turn would have been handed it; `list` prints the
//! tasks this chat dispatched; `cancel` ends one's turn and asks it for a short report. Each is
//! one ask on the chat's hook socket with no ticket, which names a chat by the app's number
//! for it. Whether that chat is this chat's task is the app's record, never anything said here
//! ([`purlis_core::dispatched`]).
//!
//! **Nothing here decides.** The checks in front of the ask are the ones that need no app: a
//! name purlis would draw, a brief that is there and carries no secret, a persona this project
//! defines. The persona is asked again by the app, with everything else.

use std::io::{IsTerminal, Read};
use std::process::ExitCode;

use purlis_core::dispatchdecision::{self, Persona, Refused};
use purlis_core::dispatched::{self, Answered, Asked, Waited, What};
use purlis_core::handback::Outcome;
use purlis_core::handoff;
use purlis_core::hookwire::{Answer, Ask, DispatchAsk, ReportBack, TaskReport};

use crate::handoff::Ticketed;
use crate::voice;

/// What `purlis dispatch`'s own lines start with.
const SAYS: &str = "purlis dispatch:";

/// What `purlis dispatch report`'s own lines start with.
const REPORT_SAYS: &str = "purlis dispatch report:";

/// What every refusal ends with: a dispatch that did not start a chat says so.
const NOTHING: &str = "Nothing was started.";

/// How to pass a brief, said by every refusal about one.
const HEREDOC: &str = "Pass the brief as a quoted heredoc in the same call:\n  purlis dispatch \
                       --name \"<task>\" <<'BRIEF'\n  <the brief>\n  BRIEF";

#[derive(clap::Subcommand, Debug)]
pub enum DispatchCommand {
    /// Send this task's one report back to the chat that dispatched it: how it ended, what
    /// you did and found, and what changed. Write your session record first; purlis adds its
    /// path. The chat that asked reads the report on its next turn.
    Report {
        /// What you did and what you found, in a few lines.
        text: String,
        /// How the task ended: done, blocked or failed.
        #[arg(long)]
        outcome: String,
        /// What changed: files, commits, a branch. A few lines at most.
        #[arg(long)]
        changed: Option<String>,
    },
    /// Wait for the report of a task this chat dispatched, and print it. `purlis dispatch
    /// list` shows each task's chat number. A wait that runs out says the task is still
    /// running; its report reaches this chat when it lands all the same.
    Wait {
        /// The task's chat, by its number in `purlis dispatch list`.
        chat: u32,
        /// How long to wait, in seconds (default 100, at most 540).
        #[arg(long)]
        timeout: Option<u32>,
    },
    /// List the tasks this chat dispatched: each one's chat number, name, persona, where it
    /// works, its state and how long ago it started.
    List,
    /// Cancel a task this chat dispatched: its turn is ended and it is asked for one short
    /// report of what it did, which arrives with the outcome `cancelled`. Only a task this
    /// chat dispatched can be cancelled by it.
    Cancel {
        /// The task's chat, by its number in `purlis dispatch list`.
        chat: u32,
    },
}

/// `purlis dispatch --name <task> [--to <persona>] [--profile <profile>] [--wait]`, with the
/// brief on stdin. `waits` is how long to wait for the report, in seconds, where the command
/// was told to wait.
pub fn dispatch(
    here: &crate::Here,
    to: Option<&str>,
    name: Option<&str>,
    profile: Option<&str>,
    waits: Option<u32>,
) -> ExitCode {
    let Some(name) = name else {
        voice::err(&format!(
            "{SAYS} a task needs a name, which the new chat is called and listed under: add \
             --name \"<task>\". {NOTHING}"
        ));
        return ExitCode::FAILURE;
    };
    // The name and the persona are asked before the read: neither needs the brief, and a
    // refusal that waited for stdin would wait on a terminal.
    if let Err(why) = checked(here, to, name) {
        voice::err(&why);
        return ExitCode::FAILURE;
    }
    let brief = match read_brief() {
        Ok(brief) => brief,
        Err(why) => {
            voice::err(&why);
            return ExitCode::FAILURE;
        }
    };
    said(send(here, to, name, &brief, profile, waits))
}

/// Prints what a dispatch or a report answered: `Ok` on stdout, `Err` on stderr as a refusal.
fn said(answered: Result<String, String>) -> ExitCode {
    match answered {
        Ok(said) => {
            println!("{said}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            voice::err(&why);
            ExitCode::FAILURE
        }
    }
}

/// The task's name as purlis will draw it, and the persona as this project defines it: the
/// two questions answered without the brief.
fn checked(here: &crate::Here, to: Option<&str>, name: &str) -> Result<String, String> {
    // A task's own rule: a chat's name, not empty, and none of the marks purlis's own lines
    // are written with. The app holds it to the same rule again.
    let name = dispatchdecision::task_name(name)
        .map_err(|why| format!("{SAYS} --name: {why} {NOTHING}"))?;
    // The app asks this again, with everything else it decides. Asked here too so a persona
    // that is not there is said before a brief is read, in the decision's own sentence.
    if let Some(to) = to {
        let refused = match dispatchdecision::persona_in(here.plane.root(), to) {
            Persona::Unknown(name) => Some(Refused::NoPersona(name.to_owned())),
            Persona::Draft(name) => Some(Refused::Draft(name.to_owned())),
            Persona::Defined(_) | Persona::None => None,
        };
        if let Some(refused) = refused {
            return Err(format!("{SAYS} {} {NOTHING}", refused.say()));
        }
    }
    Ok(name)
}

/// Dispatches the task `name` with `brief` to `to` (this chat's own persona when none is
/// named): what the command prints and the `dispatch` tool answers, or the refusal.
///
/// Every check a caller could skip is made here, so the tool, which reads no stdin, is held to
/// exactly what the command is.
///
/// With `waits`, the answer goes on to the task's report, or to where the task stands once
/// that many seconds have passed ([`wait`]): a dispatch and its result in one call (#1441).
pub fn send(
    here: &crate::Here,
    to: Option<&str>,
    name: &str,
    brief: &str,
    profile: Option<&str>,
    waits: Option<u32>,
) -> Result<String, String> {
    let name = checked(here, to, name)?;
    if !brief
        .split(purlis_core::memstore::is_python_space)
        .any(|word| !word.is_empty())
    {
        return Err(format!("{SAYS} the brief is empty. {NOTHING} {HEREDOC}"));
    }
    // The leak guard's own classifier, as a handoff's brief is held to (#1315). The KIND,
    // never the matched text.
    if let Some(kind) = purlis_core::secretshape::kind_as_read(None, brief) {
        return Err(format!(
            "{SAYS} the brief looks like it carries a secret ({kind}). {NOTHING} A brief \
             travels to the new chat as a command-line argument any local process can read \
             while the harness starts, so it never carries a secret. Name where the credential \
             lives instead of pasting it, as the whole value on its line: `vault:<vault>/<key>`."
        ));
    }
    // Measured as the new chat is sent it: under the app's two lines, with the longest name a
    // chat can have standing in for the one the app will write. The app measures again.
    let longest = "x".repeat(purlis_core::reopen::MOST_LABEL);
    let sent = handoff::task_message(
        &longest,
        &here.place(None),
        chrono::Local::now().naive_local(),
        brief,
    );
    if let Some(bad) = handoff::bad_message(&sent) {
        return Err(format!(
            "{SAYS} {} The brief is {} bytes.",
            bad.say(),
            brief.len()
        ));
    }

    let (mut asking, chat, ticket) = match crate::handoff::ticketed() {
        Ticketed::Yes(asking, chat, ticket) => (asking, chat, ticket),
        Ticketed::Refused(why) => return Err(app_refused(&ticket_words(&why))),
        Ticketed::NoApp => return Err(no_app()),
    };
    let ask = Ask::Dispatch(Box::new(DispatchAsk {
        chat,
        to: to.map(str::to_owned),
        name,
        brief: brief.to_owned(),
        profile: profile
            .map(str::trim)
            .filter(|profile| !profile.is_empty())
            .map(str::to_owned),
        ticket,
    }));
    match asking.ask(&ask, crate::handoff::AN_OPEN_TAKES_AT_MOST) {
        Ok(Answer::Dispatched {
            chat,
            name,
            persona,
            note,
        }) => {
            let who = match persona {
                Some(persona) => format!(" as {}", purlis_core::personas::one_line(&persona)),
                None => String::new(),
            };
            // What the app has to say about how it was started: today, that it runs on this
            // chat's profile because its persona's own is not offered here (D-1445-8).
            let note = match note {
                Some(note) => format!(" Note: {}.", purlis_core::personas::one_line(&note)),
                None => String::new(),
            };
            let name = purlis_core::personas::one_line(&name);
            let Some(within) = waits else {
                return Ok(format!(
                    "{SAYS} started '{name}'{who} (chat {chat}). It works in this chat's \
                     folder, and its report reaches this chat as context on its next turn.{note}"
                ));
            };
            // The chat is running either way: what the wait says is added to that, never
            // instead of it, so a wait that fails still leaves the chat's number said.
            let waited = match wait(chat, Some(within)) {
                Ok(said) | Err(said) => said,
            };
            Ok(format!(
                "{SAYS} started '{name}'{who} (chat {chat}), in this chat's folder.{note}\n{waited}"
            ))
        }
        // Held, not refused: the dispatch is accepted and waits on the person, so this is not
        // a failure and is not said as one.
        Ok(Answer::NeedsGrant { from, to, waiting }) => Ok(held_for_the_person(
            from.as_deref(),
            &to,
            &ask_name(&ask),
            waiting.as_deref(),
        )),
        Ok(Answer::No { why }) => Err(app_refused(&why)),
        Ok(
            Answer::Ticket { .. }
            | Answer::Opened { .. }
            | Answer::Reported { .. }
            | Answer::Recorded { .. }
            | Answer::Written { .. }
            | Answer::Said { .. }
            | Answer::Vaults { .. }
            | Answer::Working(_)
            | Answer::Task(_),
        )
        | Err(_) => Err(format!(
            "{SAYS} the purlis app did not answer, so purlis cannot say whether the chat \
             started. Look for '{}' under this chat in the explorer before you dispatch it \
             again.",
            purlis_core::personas::one_line(&ask_name(&ask))
        )),
    }
}

/// What a chat is told where its dispatch waits on the person for a dispatch grant: nothing
/// has started, the person is being asked on this chat's tab, and what happens next. `waiting`
/// is the task the person is already being asked about across this pair, where this ask was
/// not held beside it.
fn held_for_the_person(from: Option<&str>, to: &str, task: &str, waiting: Option<&str>) -> String {
    let one = purlis_core::personas::one_line;
    let to = one(to);
    let who = match from {
        Some(from) => format!("'{}' chats", one(from)),
        None => "this chat".to_owned(),
    };
    if let Some(first) = waiting {
        return format!(
            "{SAYS} not held. The person is already being asked whether {who} may dispatch to \
             '{to}', for the task '{}', and they were shown that task's brief, so only it \
             starts when they allow it. {NOTHING} Dispatch '{}' again once they have answered.",
            one(first),
            one(task)
        );
    }
    format!(
        "{SAYS} held for the person. {who} may not dispatch to '{to}' yet, so the person is \
         being asked on this chat's tab, with this brief in front of them. Nothing has started. \
         If they allow it, '{}' starts then and its report reaches this chat as context on a \
         later turn; if they keep it blocked, this chat is told on its next turn. Carry on \
         with other work, and do not dispatch it again.",
        one(task)
    )
}

/// The name a dispatch ask carries.
fn ask_name(ask: &Ask) -> String {
    match ask {
        Ask::Dispatch(dispatch) => dispatch.name.clone(),
        _ => String::new(),
    }
}

/// The app's refusal of a ticket, in a dispatch's words: the ticket is the handoff's own, and
/// its one sentence about a ticket still live names a handoff, whatever was being asked for.
fn ticket_words(why: &str) -> String {
    why.replacen(
        "a handoff from chat",
        "a dispatch or a handoff from chat",
        1,
    )
}

fn app_refused(why: &str) -> String {
    let why = purlis_core::personas::one_line(why);
    // A refusal that already says nothing was started is not told so twice.
    if why.to_lowercase().contains("nothing was started") {
        format!("{SAYS} {why}")
    } else {
        format!("{SAYS} {why} {NOTHING}")
    }
}

fn no_app() -> String {
    format!(
        "{SAYS} no purlis app answered this call, so nothing was started. A dispatch starts a \
         chat in the app: run it from a chat the purlis app started."
    )
}

/// `purlis dispatch report`: the one report a task owes, sent to the chat that dispatched it.
///
/// **It names no recipient**, as a handoff's report names none: the app sends it to the chat
/// it recorded as this one's asker, and adds the path of the session record it wrote for this
/// chat. What this hands over is what only the chat can say: how it ended, its words, and what
/// it says changed.
pub fn report(outcome: &str, text: &str, changed: Option<&str>) -> Result<String, String> {
    let Some(outcome) = Outcome::of(outcome) else {
        return Err(format!(
            "{REPORT_SAYS} --outcome is one of done, blocked or failed, not '{}'. Nothing was \
             sent.",
            purlis_core::personas::one_line(outcome)
        ));
    };
    let summary =
        handoff::report_summary(text).map_err(|bad| format!("{REPORT_SAYS} {}", words(&bad)))?;
    let changed = match changed {
        None => None,
        Some(changed) => Some(
            handoff::report_summary(changed)
                .map_err(|bad| format!("{REPORT_SAYS} --changed: {}", words(&bad)))?,
        ),
    };
    let (mut asking, chat, ticket) = match crate::handoff::ticketed() {
        Ticketed::Yes(asking, chat, ticket) => (asking, chat, ticket),
        Ticketed::Refused(why) => return Err(not_sent(&why)),
        Ticketed::NoApp => {
            return Err(format!(
                "{REPORT_SAYS} no purlis app answered this call, so nothing was sent. A report \
                 goes back only from a chat a dispatch started."
            ));
        }
    };
    let back = ReportBack {
        chat,
        summary,
        ticket,
        task: Some(TaskReport { outcome, changed }),
    };
    match asking.ask(
        &Ask::Report(Box::new(back)),
        crate::handoff::A_TICKET_TAKES_AT_MOST,
    ) {
        Ok(Answer::Reported { to, kept_for: None }) => Ok(format!(
            "{REPORT_SAYS} sent to '{}' ({}). It reaches that chat as context on its next turn.",
            purlis_core::personas::one_line(&to),
            outcome.word()
        )),
        Ok(Answer::Reported {
            to,
            kept_for: Some(kept),
        }) => {
            let kept_for = match purlis_core::active::Place::read(&kept) {
                Some(place) => place.said(),
                None => format!("'{}'", purlis_core::personas::one_line(&kept)),
            };
            Ok(format!(
                "{REPORT_SAYS} '{}' has closed, so the report is kept for {kept_for}. The next \
                 chat that starts there reads it.",
                purlis_core::personas::one_line(&to),
            ))
        }
        Ok(Answer::No { why }) => Err(not_sent(&why)),
        Ok(
            Answer::Ticket { .. }
            | Answer::Opened { .. }
            | Answer::Recorded { .. }
            | Answer::Written { .. }
            | Answer::Said { .. }
            | Answer::Vaults { .. }
            | Answer::Working(_)
            | Answer::Dispatched { .. }
            | Answer::NeedsGrant { .. }
            | Answer::Task(_),
        )
        | Err(_) => Err(format!(
            "{REPORT_SAYS} the purlis app did not answer, so nothing was sent."
        )),
    }
}

/// A report refusal in this command's words: the core's sentence names the handoff's own
/// spelling of the command, which a task's chat does not run.
fn words(bad: &handoff::BadReport) -> String {
    match bad {
        handoff::BadReport::Empty => "the report is empty, so nothing was sent. Say what was \
                                      done in a few lines."
            .to_owned(),
        handoff::BadReport::TooLong(_) | handoff::BadReport::Undrawable => bad.say(),
    }
}

fn not_sent(why: &str) -> String {
    format!(
        "{REPORT_SAYS} {} — nothing was sent.",
        purlis_core::personas::one_line(why)
    )
}

/// `purlis dispatch report …`, `wait`, `list` and `cancel`, printed.
pub fn run(command: &DispatchCommand) -> ExitCode {
    match command {
        DispatchCommand::Report {
            text,
            outcome,
            changed,
        } => said(report(outcome, text, changed.as_deref())),
        DispatchCommand::Wait { chat, timeout } => said(wait(*chat, *timeout)),
        DispatchCommand::List => said(list()),
        DispatchCommand::Cancel { chat } => said(cancel(*chat)),
    }
}

/// How much longer than the wait itself the app has to answer one: it answers when the wait
/// ends, and the answer is one line.
const A_WAIT_IS_ANSWERED_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

/// A conversation with the app that started this chat, and the chat's number: what an ask
/// that needs no ticket is made on.
fn the_app() -> Option<(purlis_core::hookwire::Asking, u32)> {
    use purlis_core::hookwire::{Asking, CHAT_ENV, ChatToken, SOCKET_ENV};

    let socket = purlis_core::envvar::var_os(SOCKET_ENV).filter(|s| !s.is_empty())?;
    let chat = purlis_core::envvar::var(CHAT_ENV).and_then(|chat| chat.parse::<u32>().ok())?;
    let asking = Asking::on(std::path::Path::new(&socket), ChatToken::from_env()).ok()?;
    Some((asking, chat))
}

/// One ask after a dispatched task, answered: what the app said of it, or the refusal.
fn asked(what: What, within: std::time::Duration) -> Result<Answered, String> {
    let Some((mut asking, chat)) = the_app() else {
        return Err(format!(
            "{SAYS} no purlis app answered this call. A chat's dispatched tasks are the app's \
             to say: run it from a chat the purlis app started."
        ));
    };
    answered(&mut asking, chat, what, within)
}

fn answered(
    asking: &mut purlis_core::hookwire::Asking,
    chat: u32,
    what: What,
    within: std::time::Duration,
) -> Result<Answered, String> {
    match asking.ask(&Ask::Task(Box::new(Asked { chat, what })), within) {
        Ok(Answer::Task(answered)) => Ok(*answered),
        Ok(Answer::No { why }) => Err(format!("{SAYS} {}", purlis_core::personas::one_line(&why))),
        Ok(_) | Err(_) => Err(format!(
            "{SAYS} the purlis app did not answer. `purlis dispatch list` shows where this \
             chat's tasks stand."
        )),
    }
}

/// `purlis dispatch wait <chat>`: the report of task `of`, as this chat's next turn would have
/// been handed it, or where the task stands once the wait has run out.
///
/// **A wait that runs out is an answer, not a failure**: the task is still running, its report
/// still reaches this chat when it lands, and the sentence says how to look again.
pub fn wait(of: u32, within: Option<u32>) -> Result<String, String> {
    let within = dispatched::wait_secs(within.unwrap_or(dispatched::WAITS_BY_DEFAULT));
    let Some((mut asking, chat)) = the_app() else {
        return Err(format!(
            "{SAYS} no purlis app answered this call, so there is no report to wait for here. \
             Run it from a chat the purlis app started."
        ));
    };
    let held = std::time::Duration::from_secs(u64::from(within)) + A_WAIT_IS_ANSWERED_WITHIN;
    let waited = answered(
        &mut asking,
        chat,
        What::Wait {
            of,
            within_secs: within,
        },
        held,
    )?;
    let Answered::Waited { of, name, what } = waited else {
        return Err(format!("{SAYS} the purlis app did not answer the wait."));
    };
    let name = purlis_core::personas::one_line(&name);
    Ok(match what {
        Waited::Reported { report } => {
            // Said on the same connection, so the turn after this one is not handed the same
            // report again. Unsaid, by a command that was killed here, the report is still
            // waiting for that turn: nothing is lost either way.
            let _ = answered(
                &mut asking,
                chat,
                What::Read { of },
                crate::handoff::A_TICKET_TAKES_AT_MOST,
            );
            purlis_core::handback::context(&[*report], false).unwrap_or_default()
        }
        Waited::Running { state } => format!(
            "{SAYS} '{name}' (chat {of}) has not reported after {within} seconds: it is {}. \
             It is still running, and its report reaches this chat as context when it lands. \
             `purlis dispatch wait {of}` waits again, and `purlis dispatch list` shows where \
             it stands.",
            purlis_core::personas::one_line(&state)
        ),
        Waited::AlreadyRead => format!(
            "{SAYS} '{name}' (chat {of}) reported earlier, and its report was handed to this \
             chat then. The app no longer holds its text."
        ),
        Waited::Ended => {
            format!("{SAYS} '{name}' (chat {of}) failed: its chat ended without a report.")
        }
    })
}

/// `purlis dispatch list`: the tasks this chat dispatched.
pub fn list() -> Result<String, String> {
    match asked(What::List, crate::handoff::A_TICKET_TAKES_AT_MOST)? {
        Answered::Listed { rows } => Ok(dispatched::list_text(&rows)),
        _ => Err(format!("{SAYS} the purlis app did not answer the list.")),
    }
}

/// `purlis dispatch cancel <chat>`: ends task `of`'s turn and asks it for a short report.
pub fn cancel(of: u32) -> Result<String, String> {
    match asked(What::Cancel { of }, crate::handoff::A_TICKET_TAKES_AT_MOST)? {
        Answered::Cancelling { of, name } => Ok(format!(
            "{SAYS} cancelling '{}' (chat {of}). Its turn is ended and it is asked for one \
             short report of what it did, which reaches this chat with the outcome \
             `cancelled`. If it is showing the person a prompt, that happens once the prompt \
             is answered. `purlis dispatch wait {of}` waits for the report here.",
            purlis_core::personas::one_line(&name)
        )),
        _ => Err(format!("{SAYS} the purlis app did not answer the cancel.")),
    }
}

/// The brief on stdin, or the sentence saying why there is none.
///
/// Asked of the terminal before any read, as a handoff's is: a read on a terminal blocks for
/// ever with nothing on screen to say why.
fn read_brief() -> Result<String, String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(format!(
            "{SAYS} reads its brief from stdin, and stdin here is a terminal. {NOTHING} \
             {HEREDOC}"
        ));
    }
    let mut bytes = Vec::new();
    if stdin.lock().read_to_end(&mut bytes).is_err() {
        return Err(format!(
            "{SAYS} this shell has no stdin to read a brief from. {NOTHING} {HEREDOC}"
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| format!("{SAYS} the brief on stdin is not UTF-8 text. {NOTHING}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_still_live_is_said_in_a_dispatch_s_words() {
        // The sentence the app's ticket mint says, whichever command asked for the ticket.
        let tickets = purlis_core::hookwire::Tickets::default();
        let now = std::time::Instant::now();
        tickets.mint(3, 1, now).expect("the first");
        let why = tickets.mint(3, 2, now).expect_err("one live ticket a chat");

        let said = ticket_words(&why);

        assert!(
            said.contains("a dispatch or a handoff from chat 3"),
            "{said}"
        );
        // Any other refusal is said as the app said it.
        assert_eq!(
            ticket_words("chat 3 is not one this app has open"),
            "chat 3 is not one this app has open"
        );
    }
}
