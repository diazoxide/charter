//! The app's half of what a chat may do with a task it dispatched (#1441): wait for its report,
//! list its tasks, cancel one, and be told when a report lands.
//!
//! The rules are `purlis_core::dispatched`'s, which is plain Rust with tests of its own. This
//! module feeds them the two things only the app has, and does what they answer:
//!
//! - **the app's own record of each chat** ([`crate::chats::Chats::handed_from`]), which is what
//!   says whose task a chat is. An ask names a chat by number, and nothing else it says is read;
//! - **what the board says of a chat right now** ([`seen`]): whether its turn has ended, and
//!   whether it is showing the person a prompt.
//!
//! # What is typed into a chat, and when
//!
//! Two lines, both purlis's own sentences and never a chat's words: the one that tells an
//! asking chat reports have landed (`dispatched::nudge`), and the one that asks a cancelled
//! chat for a short report (`dispatched::CANCEL_PROMPT`). Each is sent as a smart close's
//! prompt is ([`crate::smartclose`]): only into a chat that is waiting for a prompt and asking
//! nothing, and here also only while the person has no line of their own half typed in its
//! pane, since an Enter of ours would send theirs. A chat that is not ready is asked again
//! when it next moves ([`moved`]), which the hook channel says.
//!
//! The one thing sent to a chat mid-turn is a cancel's interrupt, which is the key the person
//! would press to stop it, and it too is held while the chat shows a prompt.
//!
//! # A wait holds a thread, never the listener
//!
//! [`wait`] runs on the thread the hook channel gives each connection, so the listener and
//! every other chat's lines are untouched by it. It holds the project only weakly and for a
//! bounded time (`dispatched::WAITS_AT_MOST`), so a closed project is let go of at once.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant, SystemTime};

use purlis_core::active::Place;
use purlis_core::dispatched::{self, Answered, Asked, Ledger, Row, Seen, Step, Waited, What};
use purlis_core::hookwire::{Answer, Report};
use purlis_core::reopen::HandedFrom;
use purlis_core::state::{Event, State};

use crate::planes::Held;

/// What one project's app remembers of its dispatched tasks, and who is waiting on them.
#[derive(Debug, Default)]
pub struct Tasks {
    ledger: Mutex<Ledger>,
    /// Told whenever something a wait could end on has happened.
    moved: Condvar,
}

impl Tasks {
    pub fn ledger(&self) -> MutexGuard<'_, Ledger> {
        self.ledger.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn changed(&self) {
        self.moved.notify_all();
    }
}

/// How often a wait looks again at what no notice tells it of: a chat that closed, a program
/// that ended, a project that was let go.
const A_WAIT_LOOKS_EVERY: Duration = Duration::from_millis(500);

/// Chat `chat` as the board has it now.
fn seen(held: &Held, chat: u32) -> Seen {
    let open = held
        .chats()
        .open_now()
        .iter()
        .any(|open| open.session == chat);
    let glance = held.board().glance(chat);
    Seen {
        ended: !open || matches!(glance.state, State::Done | State::Failed),
        waiting: glance.state == State::Waiting,
        asking: glance.asking,
    }
}

fn no(why: String) -> Answer {
    Answer::No { why }
}

fn task(answered: Answered) -> Answer {
    Answer::Task(Box::new(answered))
}

/// The app's record of task `of`, where the chat that asks dispatched it, and its name.
fn owned(held: &Held, asker: u32, of: u32) -> Result<(HandedFrom, String), String> {
    let record = held.chats().handed_from(of);
    let from = dispatched::owned(asker, of, record.as_ref())?.clone();
    let name = held
        .chats()
        .shown_name(of)
        .ok_or_else(|| dispatched::not_yours(of))?;
    Ok((from, name))
}

/// Answers one ask after a dispatched task that needs no waiting.
pub fn answer(held: &Held, asked: &Asked) -> Answer {
    let asker = asked.chat;
    match asked.what {
        What::Wait { .. } => wait(&held.weak(), asked),
        What::Read { of } => match owned(held, asker, of) {
            Ok(_) => {
                let file = held.tasks().ledger().read(of);
                if let Some(file) = file {
                    purlis_core::handback::took(held.root(), &file);
                }
                task(Answered::Noted)
            }
            Err(why) => no(why),
        },
        What::List => task(Answered::Listed {
            rows: list(held, asker),
        }),
        What::Cancel { of } => cancel(held, asker, of).unwrap_or_else(no),
    }
}

/// Waits for task `of`'s report on behalf of the chat that dispatched it, for as long as the
/// ask says and no longer than a wait may be.
///
/// Every look is a fresh judgement from the app's records, so a task that is closed, or a
/// project that is let go, ends the wait at once. Nothing is held across a look but the
/// ledger's own lock, and that only while it sleeps on its condition.
pub fn wait(held: &Weak<Held>, asked: &Asked) -> Answer {
    let What::Wait { of, within_secs } = asked.what else {
        return no("not a wait".to_owned());
    };
    let until = Instant::now() + Duration::from_secs(u64::from(dispatched::wait_secs(within_secs)));
    loop {
        let Some(held) = held.upgrade() else {
            return no("this project has been closed".to_owned());
        };
        let (from, name) = match owned(&held, asked.chat, of) {
            Ok(owned) => owned,
            Err(why) => return no(why),
        };
        let seen = seen(&held, of);
        let tasks = Arc::clone(held.tasks_shared());
        let ledger = tasks.ledger();
        let ended = ledger.waited(of, &from, seen).or_else(|| {
            (Instant::now() >= until).then(|| Waited::Running {
                state: ledger.state(of, &from, seen).say(),
            })
        });
        if let Some(what) = ended {
            return task(Answered::Waited { of, name, what });
        }
        // The project is let go of while this sleeps: only the ledger is kept.
        drop(held);
        let left = until.saturating_duration_since(Instant::now());
        let _ = tasks
            .moved
            .wait_timeout(ledger, left.min(A_WAIT_LOOKS_EVERY))
            .unwrap_or_else(PoisonError::into_inner);
    }
}

/// The tasks chat `asker` dispatched that are still open, in the order they were started.
fn list(held: &Held, asker: u32) -> Vec<Row> {
    let now = SystemTime::now();
    let mut open = held.chats().open_now();
    open.sort_by_key(|open| open.session);
    open.into_iter()
        .filter_map(|open| {
            let from = dispatched::owned(asker, open.session, open.from.as_ref()).ok()?;
            let seen = seen(held, open.session);
            let name = held
                .chats()
                .shown_name(open.session)
                .unwrap_or_else(|| open.name.clone());
            let ledger = held.tasks().ledger();
            Some(Row {
                chat: open.session,
                name,
                persona: open.persona.clone(),
                place: open
                    .workspace
                    .clone()
                    .map_or(Place::PlaneRoot, Place::Workspace)
                    .word()
                    .to_owned(),
                state: ledger.state(open.session, from, seen).say(),
                age_secs: ledger.age(open.session, now).map(|age| age.as_secs()),
            })
        })
        .collect()
}

/// Cancels task `of` for the chat that dispatched it: its turn is ended and it is asked for a
/// short report, as soon as it may be sent anything.
fn cancel(held: &Held, asker: u32, of: u32) -> Result<Answer, String> {
    let (from, name) = owned(held, asker, of)?;
    let began = held.tasks().ledger().cancel(of, &name, &from)?;
    if began {
        advance_cancel(held, of, false);
    }
    Ok(task(Answered::Cancelling { of, name }))
}

/// The app started the persona chat `task`.
pub fn started(held: &Held, task: u32) {
    held.tasks().ledger().started(task, SystemTime::now());
}

/// The outcome `task`'s report is delivered with, given the one its chat said.
pub fn outcome_for(
    held: &Held,
    task: u32,
    said: purlis_core::handback::Outcome,
) -> Result<purlis_core::handback::Outcome, String> {
    held.tasks().ledger().outcome_for(task, said)
}

/// Task `task`'s report was delivered for the chat `asker`: left in `file` for its next turn
/// where that chat is open. A command waiting on it has it now, and an asking chat that is
/// waiting for the person is told.
pub fn reported(
    held: &Held,
    task: u32,
    asker: u32,
    report: purlis_core::handback::Handback,
    file: Option<std::path::PathBuf>,
) {
    held.tasks().ledger().reported(task, asker, report, file);
    held.tasks().changed();
    tell_the_asker(held, asker);
}

/// Types the line that says reports have landed into chat `asker`, where there are reports it
/// was not told of and it may be sent a line now.
fn tell_the_asker(held: &Held, asker: u32) {
    if !dispatched::told_by_a_line(held.chats().harness(asker)) {
        return;
    }
    let seen = seen(held, asker);
    let typing = held.closing().person_has_a_line(asker);
    let landed = held.tasks().ledger().nudge_step(asker, seen, typing);
    if landed.is_empty() {
        return;
    }
    if let Err(why) = type_line(held, asker, &dispatched::nudge(&landed)) {
        tracing::warn!(
            "purlis: chat {asker} was not told its tasks' reports landed ({why}); they reach \
             its next turn"
        );
    }
}

/// One line and Enter, as a smart close's prompt is sent: a bracketed paste, then a carriage
/// return, in one write.
fn type_line(held: &Held, chat: u32, line: &str) -> Result<(), String> {
    let bytes = format!("{}\r", crate::curation::bracketed(line));
    held.chats().sessions().input(chat, bytes.as_bytes())
}

/// Moves task `task`'s cancel on by one step, if one is under way. `settled` says its
/// interrupted turn has had its moment to stop.
fn advance_cancel(held: &Held, task: u32, settled: bool) {
    let Some(from) = held.chats().handed_from(task) else {
        return;
    };
    let seen = seen(held, task);
    let typing = held.closing().person_has_a_line(task);
    let step = held
        .tasks()
        .ledger()
        .cancel_step(task, from.report, seen, typing, settled);
    match step {
        Step::Nothing | Step::Hold => {}
        Step::Interrupt => {
            if let Err(why) = held.chats().sessions().input(task, dispatched::INTERRUPT) {
                tracing::warn!("purlis: chat {task}'s turn could not be interrupted ({why})");
            }
            settle_later(held, task);
        }
        Step::Prompt => {
            if let Err(why) = type_line(held, task, dispatched::CANCEL_PROMPT) {
                tracing::warn!(
                    "purlis: chat {task} could not be asked for its report after a cancel ({why})"
                );
            }
        }
        Step::Write(text) => {
            let written = crate::handoff::report_for(
                held,
                task,
                text,
                purlis_core::handback::Outcome::Cancelled,
            );
            if let Err(why) = written {
                tracing::warn!("purlis: a cancelled task's report was not written ({why})");
            }
        }
    }
}

/// Asks again about task `task`'s cancel once its interrupted turn has had its moment to stop
/// (`dispatched::A_TURN_STOPS_WITHIN`). On a thread of its own, holding the project weakly.
fn settle_later(held: &Held, task: u32) {
    let held = held.weak();
    let _ = std::thread::Builder::new()
        .name("purlis-dispatch-cancel".into())
        .spawn(move || {
            std::thread::sleep(dispatched::A_TURN_STOPS_WITHIN);
            if let Some(held) = held.upgrade() {
                advance_cancel(&held, task, true);
            }
        });
}

/// A report reached the project from chat `report.chat`'s harness: whatever was waiting for
/// that chat to move is asked again.
pub fn heard(held: &Held, report: &Report) {
    if report.event == Event::UserPromptSubmit {
        held.tasks().ledger().turn_began(report.chat);
    }
    moved(held, report.chat);
}

/// Chat `chat` moved: its turn ended, a prompt of its was answered, or its program ended.
pub fn moved(held: &Held, chat: u32) {
    // Most lines are from chats nothing here waits on: asked first, so they cost one look.
    if held.tasks().ledger().waits_on(chat) {
        advance_cancel(held, chat, false);
        tell_the_asker(held, chat);
    }
    held.tasks().changed();
}

/// Chat `chat` was closed: nothing is remembered of it, and whoever waits on it looks again.
pub fn closed(held: &Held, chat: u32) {
    held.tasks().ledger().forget(chat);
    held.tasks().changed();
}
