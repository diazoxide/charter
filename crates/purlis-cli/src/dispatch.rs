//! `purlis dispatch` — start a persona chat on a task, and send a task's report back (#1436).
//!
//! ```text
//! purlis dispatch --name "<task>" [--to <persona>] [--profile <profile>]
//!                 [--in workspace:<name> | --in worktree] <<'BRIEF'
//! <the brief>
//! BRIEF
//!
//! purlis dispatch report --outcome done|blocked|failed [--changed "<what changed>"] "<text>"
//!
//! purlis dispatch --name "<task>" --wait [--timeout <seconds>] <<'BRIEF' …
//! purlis dispatch wait <chat> [--timeout <seconds>]
//! purlis dispatch list
//! purlis dispatch cancel <chat>
//!
//! purlis dispatch tell <chat> "<text>"        (the asking chat, to a task still working)
//! purlis dispatch answer <chat> "<text>"      (the asking chat, to a task's question)
//! purlis dispatch note "<text>"               (a task, to its asking chat)
//! purlis dispatch ask "<question>"            (a task, to its asking chat; it waits)
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
//! **Messages** (#1442): `tell` and `answer` go down, to a task this chat dispatched; `note`
//! and `ask` go up, and name nobody, because the chat they go to is the one the app recorded as
//! this chat's asker. Each is one ask with no ticket. Who may send to whom, the limit a minute
//! and the text's bounds are the app's to judge ([`purlis_core::dispatchtalk`]); the text is
//! checked here too, so an empty or oversized one is refused before anything is asked.
//!
//! **Nothing here decides.** The checks in front of the ask are the ones that need no app: a
//! name purlis would draw, a brief that is there and carries no secret, a persona this project
//! defines. The persona is asked again by the app, with everything else.

use std::io::{IsTerminal, Read};
use std::process::ExitCode;

use purlis_core::dispatchdecision::{self, Persona, Refused};
use purlis_core::dispatched::{self, Answered, Asked, Reply, Waited, What};
use purlis_core::dispatchtalk::{self, Kind, Message};
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
    /// works, its state and how long ago it started. A task ends at its report, and stays
    /// listed as finished until its row is cleared or this chat closes.
    List,
    /// Cancel a task this chat dispatched: its turn is ended and it is asked for one short
    /// report of what it did, which arrives with the outcome `cancelled`. Only a task this
    /// chat dispatched can be cancelled by it.
    Cancel {
        /// The task's chat, by its number in `purlis dispatch list`.
        chat: u32,
    },
    /// Send a follow-up to a task this chat dispatched that is still working. It reaches that
    /// chat's next turn, quoted as data from this chat. Refused, with the task's state, once
    /// the task has finished.
    Tell {
        /// The task's chat, by its number in `purlis dispatch list`.
        chat: u32,
        /// What to add or correct, in a few lines.
        text: String,
    },
    /// Answer the question a task this chat dispatched asked it. Only a question that task
    /// asked this chat can be answered: one it put to the person is the person's.
    Answer {
        /// The task's chat, by its number in `purlis dispatch list`.
        chat: u32,
        /// The answer, in a few lines.
        text: String,
    },
    /// From a dispatched task: send the chat that asked a progress note. It reads it on its
    /// next turn, and nothing waits on it.
    Note {
        /// Where the task has got to, in a few lines.
        text: String,
    },
    /// From a dispatched task: ask the chat that asked a question, and wait for its answer.
    /// A question only the person can answer is not asked this way: ask them in this chat.
    Ask {
        /// The question, in a few lines.
        question: String,
        /// How long to wait for the answer, in seconds (default 100, at most 540).
        #[arg(long)]
        timeout: Option<u32>,
    },
}

/// What a dispatch may say besides whom it is to, its name and its brief.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options<'a> {
    /// `--profile`: one of the project's harness profiles, and one of those the project
    /// lists for the persona where it lists any (#1509).
    pub profile: Option<&'a str>,
    /// `--in`: `worktree`, or `workspace:<name>` (#1453).
    pub place: Option<&'a str>,
}

/// `purlis dispatch --name <task> [--to <persona>] [--profile <profile>] [--in <where>]
/// [--wait]`, with the brief on stdin. `waits` is how long to wait for the report, in seconds,
/// where the command was told to wait.
pub fn dispatch(
    here: &crate::Here,
    to: Option<&str>,
    name: Option<&str>,
    options: &Options<'_>,
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
    if let Err(why) = checked(here, to, name).and_then(|_| placed(options.place)) {
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
    said(send(here, to, name, &brief, options, waits))
}

/// `--in` as the app will read it, or the refusal: one of its two words, said before a brief
/// is read. Whether the workspace is there, and whether this chat works in a repo a worktree
/// can be cut from, are the app's to say, from its own record of this chat.
fn placed(place: Option<&str>) -> Result<Option<String>, String> {
    let asked = purlis_core::dispatchplace::asked(place)
        .map_err(|refused| format!("{SAYS} {} {NOTHING}", refused.say()))?;
    Ok(asked.map(|_| place.unwrap_or_default().trim().to_owned()))
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
    options: &Options<'_>,
    waits: Option<u32>,
) -> Result<String, String> {
    let name = checked(here, to, name)?;
    let place = placed(options.place)?;
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
        Ticketed::Refused(why) => return Err(app_refused(&why)),
        Ticketed::NoApp => return Err(no_app()),
    };
    let ask = Ask::Dispatch(Box::new(DispatchAsk {
        chat,
        to: to.map(str::to_owned),
        name,
        brief: brief.to_owned(),
        profile: options
            .profile
            .map(str::trim)
            .filter(|profile| !profile.is_empty())
            .map(str::to_owned),
        place,
        ticket,
    }));
    // A worktree is cut before the chat starts: a checkout, which on a large repo takes far
    // longer than a start alone.
    let cuts_a_worktree = matches!(
        purlis_core::dispatchplace::asked(options.place),
        Ok(Some(purlis_core::dispatchplace::Where::Worktree))
    );
    let within = if cuts_a_worktree {
        A_WORKTREE_TAKES_AT_MOST
    } else {
        crate::handoff::AN_OPEN_TAKES_AT_MOST
    };
    match asking.ask(&ask, within) {
        Ok(Answer::Dispatched {
            chat,
            name,
            persona,
            note,
            works,
        }) => {
            let who = match persona {
                Some(persona) => format!(" as {}", purlis_core::personas::one_line(&persona)),
                None => String::new(),
            };
            // What the app has to say about how it was started: today, that it runs on this
            // chat's profile because its persona's own is not offered here (D-1445-8).
            // Whole, never cut ([`whole`]): since #1453 it can say more than one thing (a
            // default worktree that gave way, a clone whose uncommitted changes stayed behind).
            let note = match note {
                Some(note) => format!(" Note: {}.", whole(&note)),
                None => String::new(),
            };
            // Where it works, in the app's words: another workspace, or a worktree of its own
            // with the branch purlis cut (#1453). The app says nothing where it is this
            // chat's folder.
            let works = match works {
                Some(works) => whole(&works),
                None => "in this chat's folder".to_owned(),
            };
            let name = purlis_core::personas::one_line(&name);
            let Some(within) = waits else {
                return Ok(format!(
                    "{SAYS} started '{name}'{who} (chat {chat}). It works {works}, and its \
                     report reaches this chat as context on its next turn.{note}"
                ));
            };
            // The chat is running either way: what the wait says is added to that, never
            // instead of it, so a wait that fails still leaves the chat's number said.
            let waited = match wait(chat, Some(within)) {
                Ok(said) | Err(said) => said,
            };
            Ok(format!(
                "{SAYS} started '{name}'{who} (chat {chat}), {works}.{note}\n{waited}"
            ))
        }
        // Held, not refused: the dispatch is accepted and waits on the person, so this is not
        // a failure and is not said as one.
        Ok(Answer::NeedsGrant {
            from,
            to,
            waiting: None,
        }) => Ok(held_for_the_person(from.as_deref(), &to, &ask_name(&ask))),
        // Not held: the person is already being asked about this pair for another task, and
        // this one was dropped. Nothing of it will start, so it is a refusal.
        Ok(Answer::NeedsGrant {
            from,
            to,
            waiting: Some(first),
        }) => Err(not_held(from.as_deref(), &to, &ask_name(&ask), &first)),
        // Held, not refused: every check let it through, and it waits on this machine's memory
        // (#1467). Not a failure either.
        Ok(Answer::WaitingOnMemory { to }) => Ok(waiting_on_memory(to.as_deref(), &ask_name(&ask))),
        Ok(Answer::No { why }) => Err(app_refused(&why)),
        Ok(
            Answer::Ticket { .. }
            | Answer::Opened { .. }
            | Answer::Reported { .. }
            | Answer::Finished { .. }
            | Answer::Recorded { .. }
            | Answer::Written { .. }
            | Answer::Said { .. }
            | Answer::Vaults { .. }
            | Answer::Working(_)
            | Answer::Task(_),
        )
        | Err(_) => Err(no_answer(&ask_name(&ask), cuts_a_worktree)),
    }
}

/// How long the app has to answer a dispatch that cuts a worktree: a checkout of the repo,
/// then the chat's start. Still a bound, for the reason every wait here is one.
const A_WORKTREE_TAKES_AT_MOST: std::time::Duration = std::time::Duration::from_secs(120);

/// What a chat is told where the app did not answer in time. **It is not told nothing
/// started**: the app may still be cutting the worktree or starting the chat, and a second
/// dispatch of the same task would make a second chat.
fn no_answer(task: &str, cuts_a_worktree: bool) -> String {
    let task = purlis_core::personas::one_line(task);
    if cuts_a_worktree {
        return format!(
            "{SAYS} the purlis app did not answer in time, so purlis cannot say whether the \
             chat started. Cutting a worktree of a large repo can take longer than this \
             command waits, and the chat may still start. Look for '{task}' under this chat \
             in the explorer before you dispatch it again."
        );
    }
    format!(
        "{SAYS} the purlis app did not answer, so purlis cannot say whether the chat started. \
         Look for '{task}' under this chat in the explorer before you dispatch it again."
    )
}

/// The asking side of a pair, as a sentence names it.
fn who_asks(from: Option<&str>) -> String {
    match from {
        Some(from) => format!("'{}' chats", purlis_core::personas::one_line(from)),
        None => "this chat".to_owned(),
    }
}

/// What a chat is told where its dispatch waits on the person for a dispatch grant: nothing
/// has started, the person is being asked on this chat's tab, and what happens next.
fn held_for_the_person(from: Option<&str>, to: &str, task: &str) -> String {
    let one = purlis_core::personas::one_line;
    format!(
        "{SAYS} held for the person. {} may not dispatch to '{}' yet, so the person is \
         being asked on this chat's tab, with this brief in front of them. Nothing has started. \
         If they allow it, '{}' starts then and its report reaches this chat as context on a \
         later turn; if they keep it blocked, this chat is told on its next turn. Carry on \
         with other work, and do not dispatch it again.",
        who_asks(from),
        one(to),
        one(task)
    )
}

/// What a chat is told where its task waits on this machine's memory (#1467): nothing has
/// started, it starts by itself once memory frees, and what it hears if it never does.
fn waiting_on_memory(to: Option<&str>, task: &str) -> String {
    let one = purlis_core::personas::one_line;
    let to = to.map_or_else(String::new, |to| format!(" as '{}'", one(to)));
    format!(
        "{SAYS} waiting on memory. This machine is short on memory, so '{}'{to} has not \
         started yet. It starts by itself once memory frees, held to the limits as they are \
         then, and its report reaches this chat as context on a later turn. If memory is still \
         short after {} minutes, nothing starts and this chat is told on its next turn. Carry \
         on with other work, and do not dispatch it again.",
        one(task),
        purlis_core::dispatchdecision::MEMORY_WAIT_MINUTES
    )
}

/// What a chat is told where the person is already being asked about this pair for the task
/// `first`: this ask was not held beside it, because the person was shown one brief.
fn not_held(from: Option<&str>, to: &str, task: &str, first: &str) -> String {
    let one = purlis_core::personas::one_line;
    format!(
        "{SAYS} not held. The person is already being asked whether {} may dispatch to \
         '{}', for the task '{}', and they were shown that task's brief, so only it \
         starts when they allow it. {NOTHING} Dispatch '{}' again once they have answered.",
        who_asks(from),
        one(to),
        one(first),
        one(task)
    )
}

/// What the app said, as one line and **whole**: escaped as anything a chat may have had a
/// hand in is, and never cut. A refusal says what to do in its last sentence, and the budget
/// a name is drawn within (160 characters) would cut most of them off before it.
fn whole(said: &str) -> String {
    purlis_core::shown::one_line(said, purlis_core::shown::NO_CLIP)
}

/// The name a dispatch ask carries.
fn ask_name(ask: &Ask) -> String {
    match ask {
        Ask::Dispatch(dispatch) => dispatch.name.clone(),
        _ => String::new(),
    }
}

fn app_refused(why: &str) -> String {
    let why = whole(why);
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
        // Delivered, and the app says the task is over (#1485): the chat is told its program
        // ends with this turn, so it starts nothing it would lose. Only where the app says
        // so: a handoff's chat, a blocked task and one the person started are not ended.
        Ok(Answer::Finished { to }) => Ok(format!(
            "{REPORT_SAYS} sent to '{}' ({}). It reaches that chat as context on its next turn. \
             This task is finished: this chat's program is ended once this turn is over, so \
             start nothing more.",
            purlis_core::personas::one_line(&to),
            outcome.word()
        )),
        Ok(Answer::Reported { to, kept_for: None }) => Ok(format!(
            "{REPORT_SAYS} sent to '{}' ({}). It reaches that chat as context on its next turn.",
            purlis_core::personas::one_line(&to),
            outcome.word()
        )),
        // The person started this task from that chat's tab, and that chat is gone: the
        // report is the person's, and stays with this chat for them (D-1443-9).
        Ok(Answer::Reported {
            to,
            kept_for: Some(kept),
        }) if kept == purlis_core::handback::FOR_THE_PERSON => Ok(format!(
            "{REPORT_SAYS} '{}' has closed, and the person started this task from its tab, so \
             the report is theirs: this chat is marked as needing them, and they read it here.",
            purlis_core::personas::one_line(&to),
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
            | Answer::WaitingOnMemory { .. }
            | Answer::Task(_),
        )
        | Err(_) => Err(format!(
            "{REPORT_SAYS} the purlis app did not answer, so nothing was sent."
        )),
    }
}

/// A report refusal in this command's words, as its recorded answers have it.
fn words(bad: &handoff::BadReport) -> String {
    match bad {
        handoff::BadReport::Empty => "the report is empty, so nothing was sent. Say what was \
                                      done in a few lines."
            .to_owned(),
        handoff::BadReport::TooLong(_) | handoff::BadReport::Undrawable => bad.say(),
    }
}

fn not_sent(why: &str) -> String {
    format!("{REPORT_SAYS} {} — nothing was sent.", whole(why))
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
        DispatchCommand::Tell { chat, text } => said(tell(*chat, text)),
        DispatchCommand::Answer { chat, text } => said(answer(*chat, text)),
        DispatchCommand::Note { text } => said(note(text)),
        DispatchCommand::Ask { question, timeout } => said(ask(question, *timeout)),
    }
}

/// A message's text as the app will take it, or the refusal, said before anything is asked.
fn message(text: &str) -> Result<String, String> {
    dispatchtalk::text(text).map_err(|why| format!("{SAYS} {why}"))
}

/// One message handed to the app, and what the app said became of it.
fn sent(what: What) -> Result<(Kind, String), String> {
    match asked(what, crate::handoff::A_TICKET_TAKES_AT_MOST)? {
        Answered::Sent { kind, to } => Ok((kind, purlis_core::personas::one_line(&to))),
        _ => Err(format!(
            "{SAYS} the purlis app did not answer, so nothing was sent."
        )),
    }
}

/// `purlis dispatch tell <chat> "<text>"`: a follow-up to a task this chat dispatched.
pub fn tell(to: u32, text: &str) -> Result<String, String> {
    let text = message(text)?;
    let (_, name) = sent(What::Tell { to, text })?;
    Ok(format!(
        "{SAYS} follow-up sent to '{name}' (chat {to}). It reaches that chat's next turn, \
         quoted as data from this chat."
    ))
}

/// `purlis dispatch answer <chat> "<text>"`: the answer to a question a task asked this chat.
pub fn answer(to: u32, text: &str) -> Result<String, String> {
    let text = message(text)?;
    let (_, name) = sent(What::Answer { to, text })?;
    Ok(format!(
        "{SAYS} answer sent to '{name}' (chat {to}), which carries on with it."
    ))
}

/// `purlis dispatch note "<text>"`: a progress note to the chat that dispatched this one.
pub fn note(text: &str) -> Result<String, String> {
    let text = message(text)?;
    let (_, name) = sent(What::Note { text })?;
    Ok(format!(
        "{SAYS} progress note sent to '{name}'. It reads it on its next turn; nothing waits \
         on it, so carry on."
    ))
}

/// `purlis dispatch ask "<question>"`: a question to the chat that dispatched this one, and
/// the wait for its answer.
///
/// **This chat pauses on it.** The command holds until the answer comes and prints it, quoted
/// as data. If the wait runs out first, it says to end the turn: the answer is then handed to
/// this chat's next turn, which purlis starts when the answer lands.
pub fn ask(question: &str, within: Option<u32>) -> Result<String, String> {
    let text = message(question)?;
    let within = dispatched::wait_secs(within.unwrap_or(dispatched::WAITS_BY_DEFAULT));
    let Some((mut asking, chat)) = the_app() else {
        return Err(format!(
            "{SAYS} no purlis app answered this call, so nothing was sent. A question goes \
             back only from a chat a dispatch started."
        ));
    };
    let asked_text = text.clone();
    let asker = match answered(
        &mut asking,
        chat,
        What::Question { text },
        crate::handoff::A_TICKET_TAKES_AT_MOST,
    )? {
        Answered::Sent { to, .. } => purlis_core::personas::one_line(&to),
        _ => {
            return Err(format!(
                "{SAYS} the purlis app did not answer, so nothing was sent."
            ));
        }
    };
    let held = std::time::Duration::from_secs(u64::from(within)) + A_WAIT_IS_ANSWERED_WITHIN;
    let paused = format!(
        "{SAYS} question sent to '{asker}', which has not answered after {within} seconds. \
         This chat is paused on that question: do not guess, and end this turn now. The answer \
         is handed to this chat's next turn as context, quoted as data."
    );
    let replied = answered(
        &mut asking,
        chat,
        What::AwaitAnswer {
            within_secs: within,
        },
        held,
    );
    match replied {
        Ok(Answered::Replied {
            what:
                Reply::Answered {
                    from,
                    text,
                    by_person,
                },
        }) => {
            // On the same connection, so the next turn is not handed the same answer again.
            let _ = answered(
                &mut asking,
                chat,
                What::GotAnswer,
                crate::handoff::A_TICKET_TAKES_AT_MOST,
            );
            // The person answered it, in the purlis window (#1496): the app says so, on the
            // connection this command asked on, and the sentence is the person's own.
            if by_person {
                return Ok(dispatchtalk::person_said(
                    &dispatchtalk::PersonSaid::Answered {
                        // The sentence for a task names no number: the question is its own, quoted.
                        number: 0,
                        question: asked_text,
                        text,
                    },
                ));
            }
            Ok(dispatchtalk::said(&Message {
                kind: Kind::Answer,
                from,
                chat: 0,
                text,
            }))
        }
        Ok(Answered::Replied {
            what: Reply::AskerGone,
        }) => Ok(format!(
            "{SAYS} '{asker}', the chat that asked for this task, has closed, so the question \
             has nobody to answer it. Finish what you can and send the task's report, saying \
             what the question was."
        )),
        // The question is asked either way: an app that stopped answering leaves this chat
        // where a wait that ran out does.
        Ok(_) | Err(_) => Ok(paused),
    }
}

/// How long a turn's hook waits on the app for what the person said, for each of the ask and
/// the answer: it holds the turn, so an app that is slow costs the turn nothing, and what the
/// person said is handed to the next one.
const A_HOOK_WAITS: std::time::Duration = std::time::Duration::from_millis(750);

/// What the person said to this chat, as its turn is told it, and the means of saying the
/// turn has it ([`from_the_person`]).
pub struct FromThePerson {
    /// purlis's sentences with the person's words quoted under each, as data.
    pub told: String,
    /// The question each is about, by number: what the turn says it has.
    numbers: Vec<u32>,
    chat: u32,
    /// The connection they were read on, which the acknowledgement goes back on.
    asking: purlis_core::hookwire::Asking,
}

impl FromThePerson {
    /// **The turn has them**: said to the app, by each one's number, so they are handed to no
    /// later turn. **Called once the hook has printed its context, and not before**: a hook
    /// that is killed before it prints has said nothing here, and the next turn is handed
    /// them again. Briefly: an acknowledgement the app is slow to take costs only that.
    pub fn handed_over(mut self) {
        let has = Ask::Task(Box::new(Asked {
            chat: self.chat,
            what: What::HasFromThePerson {
                numbers: self.numbers,
            },
        }));
        let _ = self.asking.ask(&has, A_HOOK_WAITS / 3);
    }
}

/// **What the person said to this chat in the purlis window** since it last said it had any
/// (#1496): their answer to a question this task asked its asking chat, or word that they
/// answered a question one of this chat's tasks asked it. `None` for nothing, outside a chat
/// the app started, and where the app does not answer.
///
/// **Asked of the app and read from no file.** That the person said it is the app's to say,
/// on the connection it answers this chat on: a file is something another chat can write.
///
/// **Reading is not having.** The app keeps each one until the turn says it has it
/// ([`FromThePerson::handed_over`]), which the hook does after it has printed.
pub fn from_the_person() -> Option<FromThePerson> {
    let (mut asking, chat) = the_app()?;
    let asked = Ask::Task(Box::new(Asked {
        chat,
        what: What::FromThePerson,
    }));
    let Ok(Answer::Task(answered)) = asking.ask(&asked, A_HOOK_WAITS) else {
        return None;
    };
    let Answered::FromThePerson { said } = *answered else {
        return None;
    };
    let told = dispatchtalk::person_context(&said)?;
    Some(FromThePerson {
        told,
        numbers: said.iter().map(dispatchtalk::PersonSaid::number).collect(),
        chat,
        asking,
    })
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

/// The app's refusal of an ask after a task or of a message, as it is printed: one line and
/// **whole** ([`whole`]). These sentences end with what to do, and most are longer than the
/// budget a name is drawn within.
fn task_refused(why: &str) -> String {
    format!("{SAYS} {}", whole(why))
}

fn answered(
    asking: &mut purlis_core::hookwire::Asking,
    chat: u32,
    what: What,
    within: std::time::Duration,
) -> Result<Answered, String> {
    match asking.ask(&Ask::Task(Box::new(Asked { chat, what })), within) {
        Ok(Answer::Task(answered)) => Ok(*answered),
        Ok(Answer::No { why }) => Err(task_refused(&why)),
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
        Waited::Asks { question } => {
            // Read here, so the turn after this one is not handed the question again.
            let _ = answered(
                &mut asking,
                chat,
                What::Read { of },
                crate::handoff::A_TICKET_TAKES_AT_MOST,
            );
            format!(
                "{}\nThen `purlis dispatch wait {of}` waits on for its report.",
                dispatchtalk::said(&Message {
                    kind: Kind::Question,
                    from: name,
                    chat: of,
                    text: question,
                })
            )
        }
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
            "{SAYS} cancelling '{}' (chat {of}). Its report reaches this chat with the \
             outcome `cancelled`. Where purlis may send its chat keys, its turn is ended now \
             and it is asked for one short report of what it did. Where it may not (the chat \
             has shown the person a prompt this turn, the person has typed in it, or its \
             harness is one purlis does not type into), the cancel takes effect when that \
             turn ends. `purlis dispatch wait {of}` waits for the report here.",
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
    fn a_refused_message_is_said_whole_however_long_the_app_s_sentence_is() {
        // What the app answers `tell`, `answer`, `note`, `ask`, `wait` and `cancel` with when it
        // refuses. Each ends with what to do instead, and none is cut.
        let refusals = [
            purlis_core::dispatchtalk::no_question("check the queue", 9),
            purlis_core::dispatched::not_yours(9),
            purlis_core::dispatched::asked_by_the_person(9),
            purlis_core::dispatchtalk::NO_ASKING_CHAT.to_owned(),
            purlis_core::dispatchtalk::ALREADY_REPORTED.to_owned(),
            purlis_core::dispatchtalk::ASKED_BY_THE_PERSON.to_owned(),
            purlis_core::dispatched::being_stopped("check the queue", 9),
        ];
        let longest = refusals
            .iter()
            .map(|why| why.chars().count())
            .max()
            .expect("some");
        assert!(longest > purlis_core::shown::DISPLAY_LIMIT, "{longest}");
        for why in refusals {
            let said = task_refused(&why);
            assert_eq!(said, format!("{SAYS} {why}"));
            assert!(!said.contains('…') && !said.contains('\n'), "{said}");
        }
    }

    #[test]
    fn a_refusal_longer_than_a_name_s_budget_is_said_whole_and_on_one_line() {
        // The longest refusal a dispatch has: what an unattended chat with no standing grant
        // is told. It ends with what to do, which is the part a cut would lose.
        let why = purlis_core::dispatchunattended::Missing {
            asking: Some("steward".to_owned()),
            target: "devops".to_owned(),
            unreviewed: false,
        }
        .say();
        assert!(why.chars().count() > purlis_core::shown::DISPLAY_LIMIT);

        let said = app_refused(&why);

        assert_eq!(said, format!("{SAYS} {why} {NOTHING}"));
        assert!(!said.contains('…'), "{said}");
        // Still one line, whatever the app's sentence held.
        assert!(!app_refused("one\ntwo").contains('\n'));
        // And a refusal that already says nothing was started is not told so twice.
        let none = purlis_core::personaprofile::Refused::NoProfile.say();
        assert_eq!(app_refused(&none), format!("{SAYS} {none}"));
    }

    #[test]
    fn an_app_that_does_not_answer_in_time_is_never_said_to_have_started_nothing() {
        // #1453 review, fold-in 2: the app may still be cutting the worktree.
        assert_eq!(
            no_answer("check the queue", true),
            "purlis dispatch: the purlis app did not answer in time, so purlis cannot say \
             whether the chat started. Cutting a worktree of a large repo can take longer than \
             this command waits, and the chat may still start. Look for 'check the queue' under \
             this chat in the explorer before you dispatch it again."
        );
        for cuts in [true, false] {
            assert!(
                !no_answer("x", cuts)
                    .to_lowercase()
                    .contains("nothing was started")
            );
        }
        assert!(A_WORKTREE_TAKES_AT_MOST > crate::handoff::AN_OPEN_TAKES_AT_MOST);
    }

    #[test]
    fn a_ticket_the_app_will_not_mint_is_said_in_a_dispatch_s_words() {
        // A ticket is one run of a command's own (#1441): a chat that dispatches several tasks
        // in one step mints one for each, up to as many as it may have under way at once. The
        // refusal past that is the app's sentence, said as a dispatch's and whole. It names no
        // other command, so there is nothing in it to reword.
        use purlis_core::hookwire::{MOST_LIVE_TICKETS_A_CHAT, Tickets};
        let tickets = Tickets::default();
        let now = std::time::Instant::now();
        for connection in 0..u64::try_from(MOST_LIVE_TICKETS_A_CHAT).expect("a small number") {
            tickets.mint(3, connection, now).expect("under the cap");
        }
        let why = tickets.mint(3, 99, now).expect_err("the cap");

        let said = app_refused(&why);

        assert_eq!(
            said,
            format!(
                "{SAYS} chat 3 has {MOST_LIVE_TICKETS_A_CHAT} requests to the app under way at \
                 once; try again in a few seconds {NOTHING}"
            )
        );
        assert!(!said.contains("handoff"), "{said}");
        // A second ticket on one connection is refused too, and said the same way.
        let again = Tickets::default();
        again.mint(3, 1, now).expect("the first");
        let why = again
            .mint(3, 1, now)
            .expect_err("one live ticket a connection");
        assert_eq!(
            app_refused(&why),
            format!(
                "{SAYS} this connection already holds a ticket for chat 3; spend it first \
                 {NOTHING}"
            )
        );
    }
}
