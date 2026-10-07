//! The app's half of what a chat may do with a task it dispatched (#1441): wait for its report,
//! list its tasks, cancel one, and be told when a report lands.
//!
//! The rules are `purlis_core::dispatched`'s, which is plain Rust with tests of its own. This
//! module feeds them the two things only the app has, and does what they answer:
//!
//! - **the app's own record of each chat** ([`crate::chats::Chats::handed_from`]), which is what
//!   says whose task a chat is. An ask names a chat by number, and nothing else it says is read;
//! - **what the app knows of a chat right now** ([`seen`]): its harness, whether that harness
//!   has reported at all, whether a turn is under way or has ended, and whether it has shown
//!   the person a prompt.
//!
//! # What is typed into a chat, and when (D-1441-13)
//!
//! Two lines, both purlis's own sentences and never a chat's words: the one that tells a chat
//! something was left for its next turn (`dispatched::nudge`), and the one that asks a
//! cancelled chat for a short report (`dispatched::CANCEL_PROMPT`). And one key: a cancel's
//! interrupt, which is the key the person would press to stop a turn.
//!
//! **Each goes to a pane only where purlis knows what the pane is showing.** All of:
//!
//! - the chat's harness is one purlis has measured its line in (Claude Code; Codex once
//!   purlis's hooks are trusted there, which is when it reports at all);
//! - the board has heard from the chat since it started. A chat nothing has reported for may
//!   be showing a start-up dialog, a login, anything, and an Enter of purlis's would answer it;
//! - it has shown the person no prompt this turn;
//! - no key of the person's has gone to its pane since its harness last said a turn began or
//!   ended ([`person_typed`]). A harness's own picker, opened by a local command, is reported
//!   by no hook, and the person's key is the only sign of it.
//!
//! A line further waits for the chat's turn to have ended, and the interrupt goes only into a
//! turn purlis heard begin. Anywhere else nothing is typed: a report or a message waits for the
//! chat's next turn, as it always did, and a cancel is recorded and takes effect when the
//! chat's turn or program ends. A chat that is not ready is looked at again when it next
//! moves ([`moved`]), which the hook channel says.
//!
//! No write to a pane is made under `Chats::deciding()`: a program that has stopped reading
//! its terminal must not hold up every dispatch and report in the project.
//!
//! # Messages (#1442)
//!
//! A follow-up, a progress note, a question and an answer are `purlis_core::dispatchtalk`'s:
//! who may send to whom is the app's record of each chat, the text is held to a report's rule,
//! and each pair is limited to the project's messages a minute. A message is left in the
//! project for the chat it is for and handed to its next turn by its own hook, quoted as data.
//! Nothing a chat wrote is ever typed.
//!
//! **A question for the person never passes through here.** It is a prompt in the task's own
//! tab, which only the person's keys answer; an answer from the asking chat is refused unless
//! this module holds a question that task asked it.
//!
//! **The person's words in a task's pane are remembered as one bit** ([`person_typed`]), which
//! its report carries as "the operator stepped in". Nothing they typed is kept.
//!
//! # A wait holds a thread, never the listener (D-1441-14)
//!
//! [`wait`] runs on the thread the hook channel gives each connection, so the listener and
//! every other chat's lines are untouched by it. It holds the project only weakly and for a
//! bounded time (`dispatched::WAITS_AT_MOST`). One chat may have [`MOST_WAITS_A_CHAT`] waits
//! parked at once, and a wait whose command has hung up ends at its next look, so a process
//! inside a chat cannot spend the app's threads.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant, SystemTime};

use purlis_core::active::Place;
use purlis_core::dispatched::{
    self, Answered, Asked, Landed, Ledger, Reply, Row, Seen, Step, Waited, What,
};
use purlis_core::dispatchtalk::{self, Kind, Message};
use purlis_core::hookwire::{Answer, Report};
use purlis_core::reopen::{HandedFrom, Mode, Owed};
use purlis_core::state::{Event, State};

use crate::planes::Held;

/// What one project's app remembers of its dispatched tasks, and who is waiting on them.
#[derive(Debug, Default)]
pub struct Tasks {
    ledger: Mutex<Ledger>,
    /// Told whenever something a wait could end on has happened.
    moved: Condvar,
    /// By chat: how many waits of its are parked on a thread now.
    parked: Mutex<HashMap<u32, usize>>,
}

/// The most waits one chat may have parked at once, as it may hold that many live tickets
/// (`hookwire::MOST_LIVE_TICKETS_A_CHAT`): room for a wait on every task it can have running.
pub const MOST_WAITS_A_CHAT: usize = 16;

impl Tasks {
    pub fn ledger(&self) -> MutexGuard<'_, Ledger> {
        self.ledger.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn changed(&self) {
        self.moved.notify_all();
    }

    /// How many waits of chat `chat`'s are parked now, for a test that has to know.
    #[cfg(test)]
    pub(crate) fn parked_now(&self, chat: u32) -> usize {
        self.parked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&chat)
            .copied()
            .unwrap_or_default()
    }

    /// A place among chat `chat`'s parked waits, let go when what this answers is dropped; or
    /// the refusal where it has [`MOST_WAITS_A_CHAT`] already.
    fn park(self: &Arc<Self>, chat: u32) -> Result<Parked, String> {
        let mut parked = self.parked.lock().unwrap_or_else(PoisonError::into_inner);
        let count = parked.entry(chat).or_default();
        if *count >= MOST_WAITS_A_CHAT {
            return Err(format!(
                "chat {chat} has {MOST_WAITS_A_CHAT} waits under way already, and that is the \
                 most one chat may have. Let one end first; `purlis dispatch list` shows where \
                 each task stands without waiting."
            ));
        }
        *count += 1;
        Ok(Parked {
            tasks: Arc::clone(self),
            chat,
        })
    }
}

/// One parked wait of a chat's.
struct Parked {
    tasks: Arc<Tasks>,
    chat: u32,
}

impl Drop for Parked {
    fn drop(&mut self) {
        let mut parked = self
            .tasks
            .parked
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = parked.get_mut(&self.chat) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                parked.remove(&self.chat);
            }
        }
    }
}

/// How often a wait looks again at what no notice tells it of: a chat that closed, a program
/// that ended, a command that hung up, a project that was let go.
const A_WAIT_LOOKS_EVERY: Duration = Duration::from_millis(500);

/// Chat `chat` as the app knows it now.
fn seen(held: &Held, chat: u32) -> Seen {
    let open = held
        .chats()
        .open_now()
        .iter()
        .any(|open| open.session == chat);
    let glance = held.board().glance(chat);
    Seen {
        ended: !open || matches!(glance.state, State::Done | State::Failed),
        heard: glance.state != State::Unknown,
        // A turn the board heard begin, which has shown the person no prompt: a prompt mid-turn
        // moves the board to waiting, with `asking` set until the turn ends.
        running: glance.state == State::Running,
        waiting: glance.state == State::Waiting,
        asking: glance.asking,
        measured: dispatched::told_by_a_line(held.chats().harness(chat)),
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

/// Answers one ask after a dispatched task. `connection` is the listener's number for the
/// connection it came on, which a wait asks after.
pub fn answer(held: &Held, asked: &Asked, connection: u64) -> Answer {
    let asker = asked.chat;
    match &asked.what {
        What::Wait { .. } => wait(&held.weak(), asked, connection),
        What::AwaitAnswer { .. } => await_answer(&held.weak(), asked, connection),
        What::Read { of } => {
            // A closed task's report is read from the app's memory, and waits in no file.
            if owned(held, asker, *of).is_err() && held.tasks().ledger().gone(asker, *of).is_none()
            {
                return no(dispatched::not_yours(*of));
            }
            let files = held.tasks().ledger().read(*of);
            for file in files {
                purlis_core::handback::took(held.root(), &file);
            }
            task(Answered::Noted)
        }
        What::List => task(Answered::Listed {
            rows: list(held, asker),
        }),
        What::Cancel { of } => cancel(held, asker, *of).unwrap_or_else(no),
        What::Tell { to, text } => tell(held, asker, *to, text).unwrap_or_else(no),
        What::Note { text } => send_up(held, asker, Kind::Note, text).unwrap_or_else(no),
        What::Question { text } => send_up(held, asker, Kind::Question, text).unwrap_or_else(no),
        What::Answer { to, text } => answer_it(held, asker, *to, text).unwrap_or_else(no),
        What::GotAnswer => {
            let file = held.tasks().ledger().got_answer(asker);
            if let Some(file) = file {
                purlis_core::handback::took(held.root(), &file);
            }
            task(Answered::Noted)
        }
    }
}

/// The name the person sees chat `chat` under, as a message says it is from.
fn shown(held: &Held, chat: u32) -> Result<String, String> {
    held.chats()
        .shown_name(chat)
        .map(|name| dispatchtalk::sender_of(&name))
        .ok_or_else(|| format!("chat {chat} is not one this app has open"))
}

fn kept(why: std::io::Error) -> String {
    format!("the message could not be kept ({why})")
}

/// **The limits in force between asking chat `asker` and its task `task`** (#1439): the
/// project's files as they stand now, for the workspace the asking chat works in and the two
/// personas, under this machine's policy. The one place a message's limit is read.
fn limits_between(held: &Held, asker: u32, task: u32) -> purlis_core::dispatchlimits::Limits {
    let open = held.chats().open_now();
    let of = |chat: u32| open.iter().find(|open| open.session == chat);
    purlis_core::dispatchlimits::of(
        held.root(),
        of(asker).and_then(|open| open.workspace.as_deref()),
        of(asker).and_then(|open| open.persona.as_deref()),
        of(task).and_then(|open| open.persona.as_deref()),
    )
}

/// A follow-up from chat `asker` to task `to`: refused unless `asker` dispatched it and it is
/// still working and not being cancelled, then left for its next turn.
fn tell(held: &Held, asker: u32, to: u32, said: &str) -> Result<Answer, String> {
    let (from, name) = owned(held, asker, to)?;
    let seen = seen(held, to);
    let state = held.tasks().ledger().state(to, &from, seen);
    if seen.ended || state == dispatched::State::Cancelling {
        return Err(format!(
            "'{name}' (chat {to}) has finished: it is {}. A message would reach no turn of its \
             work. Dispatch a new task for more.",
            state.say()
        ));
    }
    dispatchtalk::still_working(&name, to, &from, &state.say())?;
    let text = dispatchtalk::text(said)?;
    let message = Message {
        kind: Kind::FollowUp,
        from: shown(held, asker)?,
        chat: asker,
        text,
    };
    let limits = limits_between(held, asker, to);
    held.tasks()
        .ledger()
        .talk
        .count(asker, to, &limits, Instant::now())?;
    dispatchtalk::leave(held.root(), to, &message).map_err(kept)?;
    held.tasks().ledger().landed(to, Landed::FollowUp);
    // One more message on the task's dispatch record (#1452), now that it is kept.
    crate::dispatches::message(held, to);
    tell_the_chat(held, to);
    Ok(task(Answered::Sent {
        kind: Kind::FollowUp,
        to: name,
    }))
}

/// A progress note or a question from task `sender` to the chat that dispatched it. The
/// recipient is the app's record of `sender`, never anything the message says.
fn send_up(held: &Held, sender: u32, kind: Kind, said: &str) -> Result<Answer, String> {
    let record = held.chats().handed_from(sender);
    let from = dispatchtalk::up(record.as_ref())?;
    if from.report != Owed::Due {
        return Err(dispatchtalk::ALREADY_REPORTED.to_owned());
    }
    let asker = from.chat;
    if seen(held, asker).ended {
        return Err(format!(
            "'{}', the chat that asked for this task, has closed, so nobody is left to read a \
             {}. Finish what you can and send the task's report, which is kept for its \
             workspace.",
            from.name,
            kind.word()
        ));
    }
    let text = dispatchtalk::text(said)?;
    let message = Message {
        kind,
        from: shown(held, sender)?,
        chat: sender,
        text: text.clone(),
    };
    let limits = limits_between(held, asker, sender);
    {
        // One hold from "has it a question open" to "this is its question", so of two
        // questions sent at once one is asked and the other refused with nothing left behind.
        let mut ledger = held.tasks().ledger();
        if kind == Kind::Question && ledger.talk.asks(sender).is_some() {
            return Err(
                "this chat already has a question waiting for its asking chat's answer, and is \
                 paused on it. Nothing was sent."
                    .to_owned(),
            );
        }
        ledger.talk.count(asker, sender, &limits, Instant::now())?;
        let file = dispatchtalk::leave(held.root(), asker, &message).map_err(kept)?;
        if kind == Kind::Question {
            ledger.talk.ask(sender, &text, Some(file))?;
            ledger.landed(asker, Landed::Question(sender));
        }
    }
    // Kept, so it counts on this task's dispatch record (#1452): a note as a question does.
    crate::dispatches::message(held, sender);
    if kind == Kind::Question {
        // A command of the asking chat's that waits on this task is answered with it; a chat
        // that is waiting for the person is typed the line. A note is neither: it is read on
        // the asking chat's next turn, and starts none.
        held.tasks().changed();
        tell_the_chat(held, asker);
    }
    Ok(task(Answered::Sent {
        kind,
        to: held
            .chats()
            .shown_name(asker)
            .unwrap_or_else(|| from.name.clone()),
    }))
}

/// Chat `asker` answers the question task `to` asked it. Refused unless `asker` dispatched
/// `to`, `to` is still working, and the app holds a question `to` asked that is still open.
///
/// **Nothing an answer says is typed into `to`**, whatever its tab is showing: the answer is
/// left for its next turn, and handed to the command of its that waits. A prompt the task is
/// showing the person is not a question held here, so it cannot be answered this way.
fn answer_it(held: &Held, asker: u32, to: u32, said: &str) -> Result<Answer, String> {
    let (from, name) = owned(held, asker, to)?;
    if from.report != Owed::Due || held.tasks().ledger().talk.asks(to).is_none() {
        return Err(dispatchtalk::no_question(&name, to));
    }
    let text = dispatchtalk::text(said)?;
    let asker_name = shown(held, asker)?;
    let message = Message {
        kind: Kind::Answer,
        from: asker_name.clone(),
        chat: asker,
        text: text.clone(),
    };
    let limits = limits_between(held, asker, to);
    {
        let mut ledger = held.tasks().ledger();
        if ledger.talk.asks(to).is_none() {
            return Err(dispatchtalk::no_question(&name, to));
        }
        ledger.talk.count(asker, to, &limits, Instant::now())?;
        let file = dispatchtalk::leave(held.root(), to, &message).map_err(kept)?;
        let given = dispatchtalk::Given {
            from: asker_name,
            text,
            file: Some(file),
        };
        ledger.talk.answer(to, &name, given)?;
        ledger.landed(to, Landed::Answer);
    }
    crate::dispatches::message(held, to);
    held.tasks().changed();
    tell_the_chat(held, to);
    Ok(task(Answered::Sent {
        kind: Kind::Answer,
        to: name,
    }))
}

/// Waits for the answer to the question task `asked.chat` asked its asking chat, for as long
/// as the ask says and no longer than a wait may be. Held and bounded as [`wait`] is.
pub fn await_answer(held: &Weak<Held>, asked: &Asked, connection: u64) -> Answer {
    let What::AwaitAnswer { within_secs } = asked.what else {
        return no("not a wait".to_owned());
    };
    let sender = asked.chat;
    let until = Instant::now() + Duration::from_secs(u64::from(dispatched::wait_secs(within_secs)));
    let mut parked = None;
    loop {
        let Some(held) = held.upgrade() else {
            return no("this project has been closed".to_owned());
        };
        let record = held.chats().handed_from(sender);
        let from = match dispatchtalk::up(record.as_ref()) {
            Ok(from) => from.clone(),
            Err(why) => return no(why),
        };
        let tasks = Arc::clone(held.tasks_shared());
        if parked.is_none() {
            match tasks.park(sender) {
                Ok(place) => parked = Some(place),
                Err(why) => return no(why),
            }
        }
        let asker_gone = seen(&held, from.chat).ended;
        let hung_up = held.hooks().asker_gone(connection);
        let ledger = tasks.ledger();
        if let Some(given) = ledger.talk.answered(sender) {
            return task(Answered::Replied {
                what: Reply::Answered {
                    from: given.from.clone(),
                    text: given.text.clone(),
                },
            });
        }
        if ledger.talk.asks(sender).is_none() {
            return no("this chat has no question waiting for an answer".to_owned());
        }
        if asker_gone {
            return task(Answered::Replied {
                what: Reply::AskerGone,
            });
        }
        if hung_up || Instant::now() >= until {
            return task(Answered::Replied {
                what: Reply::NotYet { from: from.name },
            });
        }
        drop(held);
        let left = until.saturating_duration_since(Instant::now());
        let _ = tasks
            .moved
            .wait_timeout(ledger, left.min(A_WAIT_LOOKS_EVERY))
            .unwrap_or_else(PoisonError::into_inner);
    }
}

/// The person sent `bytes` from chat `chat`'s pane.
///
/// **Any key of theirs holds back what purlis would type into that chat**, until its harness
/// next says a turn began or ended: they may have opened a picker of the harness's own, which
/// no hook reports.
///
/// **And in a task still working, their words are them stepping in** (`dispatched::steps_in`):
/// its report will say so. The fact is kept, and nothing of the bytes. `line` is how many
/// bytes they have typed since their last Enter, where that could be followed.
pub fn person_typed(held: &Held, chat: u32, bytes: &[u8], line: Option<usize>) {
    if !crate::smartclose::typed_by_the_operator(bytes) {
        return;
    }
    let stepped_in = {
        let mut ledger = held.tasks().ledger();
        ledger.person_keyed(chat);
        ledger.stepped_in(chat)
    };
    if stepped_in {
        return;
    }
    let owes = held
        .chats()
        .handed_from(chat)
        .is_some_and(|from| from.mode == Mode::Task && from.report == Owed::Due);
    if owes && dispatched::steps_in(held.board().glance(chat).asking, line, bytes) {
        held.tasks().ledger().person_typed(chat);
    }
}

/// Whether the person typed in task `task`'s pane while it worked.
pub fn stepped_in(held: &Held, task: u32) -> bool {
    held.tasks().ledger().stepped_in(task)
}

/// Waits for task `of`'s report on behalf of the chat that dispatched it, for as long as the
/// ask says and no longer than a wait may be.
///
/// Every look is a fresh judgement from the app's records, so a task that is closed, or a
/// project that is let go, ends the wait at once; and so does a command that hung up
/// (`connection`'s asker gone). Nothing is held across a look but the ledger's own lock, and
/// that only while it sleeps on its condition.
pub fn wait(held: &Weak<Held>, asked: &Asked, connection: u64) -> Answer {
    let What::Wait { of, within_secs } = asked.what else {
        return no("not a wait".to_owned());
    };
    let until = Instant::now() + Duration::from_secs(u64::from(dispatched::wait_secs(within_secs)));
    let mut parked = None;
    loop {
        let Some(held) = held.upgrade() else {
            return no("this project has been closed".to_owned());
        };
        let tasks = Arc::clone(held.tasks_shared());
        let (from, name) = match owned(&held, asked.chat, of) {
            Ok(owned) => owned,
            // Its tab was closed: the chat that asked is told how it ended, not that the
            // task was never its own.
            Err(why) => {
                let ledger = tasks.ledger();
                let Some(gone) = ledger.gone(asked.chat, of) else {
                    return no(why);
                };
                return task(Answered::Waited {
                    of,
                    name: gone.name.clone(),
                    what: match &gone.report {
                        Some(report) => Waited::Reported {
                            report: Box::new(report.clone()),
                        },
                        None => Waited::Ended,
                    },
                });
            }
        };
        if parked.is_none() {
            match tasks.park(asked.chat) {
                Ok(place) => parked = Some(place),
                Err(why) => return no(why),
            }
        }
        let seen = seen(&held, of);
        let hung_up = held.hooks().asker_gone(connection);
        let mut ledger = tasks.ledger();
        let ended = ledger.waited(of, &from, seen).or_else(|| {
            (hung_up || Instant::now() >= until).then(|| Waited::Running {
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

/// The tasks under chat `asker` that are still open, in the order they were started.
///
/// **One list, read from the one place that says which chats those are and where each stands**
/// ([`crate::handoff::persona_chats`], #1443, D-T59-j2): what closing the chat asks about, what
/// the window marks a task with and what `purlis dispatch list` prints are the same chats in
/// the same words. A task the chat dispatched itself says more where the app knows more: that
/// it is idle, cancelling, asking this chat a question, or how it reported. A task the person
/// started from this chat's tab is listed too, by name and standing, and marked as theirs:
/// the list gives the chat nothing over it ([`dispatched::owned`]).
fn list(held: &Held, asker: u32) -> Vec<Row> {
    use crate::handoff::PersonaChatState;

    let now = SystemTime::now();
    let open = held.chats().open_now();
    crate::handoff::persona_chats(held, asker)
        .into_iter()
        .filter_map(|listed| {
            let open = open.iter().find(|open| open.session == listed.session)?;
            let from = open.from.as_ref()?;
            let ledger = held.tasks().ledger();
            let own = dispatched::owned(asker, listed.session, Some(from)).is_ok();
            let state = match (own, listed.state) {
                // Waiting on the person, by the board or by an ask the app holds open: the
                // standing knows both, and is the word for it. A cancel under way is said
                // first: that is what this chat asked for.
                // Asked of the hold this row is built under: the ledger's lock is not one a
                // thread may take twice.
                (true, PersonaChatState::WaitingOnOperator)
                    if !ledger.cancelling(listed.session) =>
                {
                    listed.said.clone()
                }
                (true, _) => ledger
                    .state(listed.session, from, seen(held, listed.session))
                    .say(),
                (false, _) => listed.said.clone(),
            };
            Some(Row {
                chat: listed.session,
                name: listed.name,
                persona: listed.persona,
                place: open
                    .workspace
                    .clone()
                    .map_or(Place::PlaneRoot, Place::Workspace)
                    .word()
                    .to_owned(),
                state,
                age_secs: ledger.age(listed.session, now).map(|age| age.as_secs()),
                by_person: !own,
            })
        })
        .collect()
}

/// Cancels task `of` for the chat that dispatched it: the cancel is recorded, and its turn is
/// ended and it is asked for a short report as soon as purlis may send its pane anything.
fn cancel(held: &Held, asker: u32, of: u32) -> Result<Answer, String> {
    let (began, name) = {
        // Under the lock a report is taken under, so a cancel and a report in flight land in
        // one order: either the task has reported and the cancel is refused, or the cancel is
        // recorded first and the report is delivered as cancelled.
        let _deciding = held.chats().deciding();
        let (from, name) = owned(held, asker, of)?;
        // The person is stopping it already: that ends it and tells this chat, and a chat in
        // a stop is not cancelled as well (D-T59-j3).
        if held.stopping().is_stopping(of) {
            return Err(dispatched::being_stopped(&name, of));
        }
        (held.tasks().ledger().cancel(of, &name, &from)?, name)
    };
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
/// where that chat is open. A command waiting on it has it now. **Nothing is typed here**: it
/// is called under the lock a report is taken under, and [`told`] follows once that is let go.
pub fn reported(
    held: &Held,
    task: u32,
    asker: u32,
    report: purlis_core::handback::Handback,
    file: Option<std::path::PathBuf>,
) {
    held.tasks().ledger().reported(task, asker, report, file);
    held.tasks().changed();
}

/// Chat `chat` may now be told what was left for its next turn: typed purlis's one line, where
/// it may be sent one. Never called under `Chats::deciding()`.
pub fn told(held: &Held, chat: u32) {
    tell_the_chat(held, chat);
}

/// Types the line that says something was left for its next turn into chat `chat`, where
/// there is something it was not told of and it may be sent a line now: the reports and
/// questions of tasks it dispatched, and a message from the chat that dispatched it. Answers
/// whether a line was typed.
fn tell_the_chat(held: &Held, chat: u32) -> bool {
    // A chat the person is stopping is typed its stop's one line and no other.
    if held.stopping().is_stopping(chat) {
        return false;
    }
    let seen = seen(held, chat);
    let landed = held.tasks().ledger().nudge_step(chat, seen);
    if landed.is_empty() {
        return false;
    }
    if let Err(why) = type_line(held, chat, &dispatched::nudge(&landed)) {
        tracing::warn!(
            "purlis: chat {chat} was not told what was left for it ({why}); it reaches its \
             next turn"
        );
        return false;
    }
    true
}

/// One line and Enter, as a smart close's prompt is sent: a bracketed paste, then a carriage
/// return, in one write.
fn type_line(held: &Held, chat: u32, line: &str) -> Result<(), String> {
    let bytes = format!("{}\r", crate::curation::bracketed(line));
    held.chats().sessions().input(chat, bytes.as_bytes())
}

/// Moves task `task`'s cancel on by one step, if one is under way. `settled` says its
/// interrupted turn has just had its moment to stop. Answers whether a key went to its pane.
fn advance_cancel(held: &Held, task: u32, settled: bool) -> bool {
    let Some(from) = held.chats().handed_from(task) else {
        return false;
    };
    // The person's stop is the later word: nothing more is sent for a cancel (D-T59-j3).
    if held.stopping().is_stopping(task) {
        return false;
    }
    let seen = seen(held, task);
    let step = held
        .tasks()
        .ledger()
        .cancel_step(task, from.report, seen, settled);
    match step {
        Step::Nothing | Step::Hold => false,
        Step::Interrupt => {
            if let Err(why) = held.chats().sessions().input(task, dispatched::INTERRUPT) {
                tracing::warn!("purlis: chat {task}'s turn could not be interrupted ({why})");
            }
            look_again_after(
                held,
                task,
                dispatched::A_TURN_STOPS_WITHIN,
                After::Interrupt,
            );
            true
        }
        Step::Prompt => {
            if let Err(why) = type_line(held, task, dispatched::CANCEL_PROMPT) {
                tracing::warn!(
                    "purlis: chat {task} could not be asked for its report after a cancel ({why})"
                );
            }
            look_again_after(
                held,
                task,
                dispatched::A_PROMPT_IS_HEARD_WITHIN,
                After::Prompt,
            );
            true
        }
        Step::Write(text) => {
            write_cancelled(held, task, text);
            false
        }
    }
}

/// Writes the report of a cancelled task whose chat will send none.
fn write_cancelled(held: &Held, task: u32, text: &str) {
    let written =
        crate::handoff::report_for(held, task, text, purlis_core::handback::Outcome::Cancelled);
    if let Err(why) = written {
        tracing::warn!("purlis: a cancelled task's report was not written ({why})");
    }
}

/// What a cancel is looked at again after.
#[derive(Clone, Copy)]
enum After {
    /// Its turn was interrupted, and has had its moment to stop.
    Interrupt,
    /// It was typed the line, and no turn was heard to begin on it.
    Prompt,
}

/// Looks at task `task`'s cancel again in `after`. On a thread of its own, holding the project
/// weakly.
fn look_again_after(held: &Held, task: u32, after: Duration, what: After) {
    let held = held.weak();
    let _ = std::thread::Builder::new()
        .name("purlis-dispatch-cancel".into())
        .spawn(move || {
            std::thread::sleep(after);
            let Some(held) = held.upgrade() else { return };
            match what {
                After::Interrupt => {
                    advance_cancel(&held, task, true);
                }
                After::Prompt => {
                    held.tasks().ledger().prompt_unheard(task);
                    advance_cancel(&held, task, false);
                }
            }
        });
}

/// A report reached the project from chat `report.chat`'s harness: whatever was waiting for
/// that chat to move is looked at again.
pub fn heard(held: &Held, report: &Report) {
    match report.event {
        Event::UserPromptSubmit => held.tasks().ledger().turn_began(report.chat),
        Event::Stop => held.tasks().ledger().turn_ended(report.chat),
        _ => {}
    }
    moved(held, report.chat);
}

/// Chat `chat` moved: its turn ended, a prompt of its was answered, or its program ended.
pub fn moved(held: &Held, chat: u32) {
    // Most lines are from chats nothing here waits on: asked first, so they cost one look.
    if held.tasks().ledger().waits_on(chat) {
        // One thing a move: a chat just sent a cancel's key is not typed a second line on
        // top of it.
        if !advance_cancel(held, chat, false) {
            tell_the_chat(held, chat);
        }
    }
    held.tasks().changed();
}

/// Chat `chat` was closed. `task_of` is its asking chat and its name, where it was a task,
/// read before its record went: that much is kept, so a wait on it says how it ended. Whoever
/// waits on it looks again.
pub fn closed(held: &Held, chat: u32, task_of: Option<(u32, String)>) {
    held.tasks().ledger().forget(
        chat,
        task_of
            .as_ref()
            .map(|(asker, name)| (*asker, name.as_str())),
    );
    // A message is for one chat's turn, and this chat will have no more.
    dispatchtalk::forget(held.root(), chat);
    held.tasks().changed();
}

/// Chat `old` is now `new`: the same chat, started again under a new number. What the app
/// remembers of it, and the messages waiting for its next turn, follow it.
pub fn followed(held: &Held, old: u32, new: u32) {
    held.tasks().ledger().followed(old, new);
    dispatchtalk::moved(held.root(), old, new);
    held.tasks().changed();
}
