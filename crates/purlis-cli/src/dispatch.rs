//! `purlis dispatch` — start a persona chat on a task, and send a task's report back (#1436).
//!
//! ```text
//! purlis dispatch --name "<task>" [--to <persona>] [--profile <profile>] <<'BRIEF'
//! <the brief>
//! BRIEF
//!
//! purlis dispatch report --outcome done|blocked|failed [--changed "<what changed>"] "<text>"
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
//! **Nothing here decides.** The checks in front of the ask are the ones that need no app: a
//! name purlis would draw, a brief that is there and carries no secret, a persona this project
//! defines. The persona is asked again by the app, with everything else.

use std::io::{IsTerminal, Read};
use std::process::ExitCode;

use purlis_core::dispatchdecision::{self, Persona, Refused};
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
}

/// `purlis dispatch --name <task> [--to <persona>] [--profile <profile>]`, with the brief on
/// stdin.
pub fn dispatch(
    here: &crate::Here,
    to: Option<&str>,
    name: Option<&str>,
    profile: Option<&str>,
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
    said(send(here, to, name, &brief, profile))
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
pub fn send(
    here: &crate::Here,
    to: Option<&str>,
    name: &str,
    brief: &str,
    profile: Option<&str>,
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
        Ticketed::Refused(why) => return Err(app_refused(&why)),
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
                Some(note) => format!(" Note: {}.", whole(&note)),
                None => String::new(),
            };
            Ok(format!(
                "{SAYS} started '{}'{who} (chat {chat}). It works in this chat's folder, and \
                 its report reaches this chat as context on its next turn.{note}",
                purlis_core::personas::one_line(&name)
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
        Ok(Answer::No { why }) => Err(app_refused(&why)),
        Ok(
            Answer::Ticket { .. }
            | Answer::Opened { .. }
            | Answer::Reported { .. }
            | Answer::Recorded { .. }
            | Answer::Written { .. }
            | Answer::Said { .. }
            | Answer::Vaults { .. }
            | Answer::Working(_),
        )
        | Err(_) => Err(format!(
            "{SAYS} the purlis app did not answer, so purlis cannot say whether the chat \
             started. Look for '{}' under this chat in the explorer before you dispatch it \
             again.",
            purlis_core::personas::one_line(&ask_name(&ask))
        )),
    }
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
            | Answer::NeedsGrant { .. },
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

/// `purlis dispatch report …`, printed.
pub fn run(command: &DispatchCommand) -> ExitCode {
    match command {
        DispatchCommand::Report {
            text,
            outcome,
            changed,
        } => said(report(outcome, text, changed.as_deref())),
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
