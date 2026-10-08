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
//! # A task ends at its report (#1485)
//!
//! Once a task's report is delivered, purlis ends its program ([`delivered`], [`end_look`]):
//! by the chat's own close, so everything a close settles is settled the one way. **The
//! report first, the end after**: a task becomes one to end only where its report was kept for
//! the chat that asked (`Ledger::reported`), and the end is made on a thread of its own, which
//! takes `Chats::deciding()` as any close does. So a report in flight and the end are ordered
//! by the lock that already orders a report against an exit, and the exit that follows finds
//! nothing owed and says nothing.
//!
//! **When** is the core's rule (`Ledger::end_step`): after the turn that reported has ended,
//! by the harness's `Stop`, and had a moment for the harness to finish writing its
//! conversation down; and after a bounded wait for a turn that does not end, or a harness
//! purlis hears nothing from. Never while a task of its own is at work, never while the person
//! is stopping it (the stop ends it), and never as the app quits.
//!
//! **And never a chat that is working again, mid-turn.** A key of the person's in its pane
//! after the report ([`person_typed`]), or a Smart close of it beginning ([`stand_down`]),
//! stands the end down for good, and its record says so for the next launch. A line purlis
//! types into it starts a turn that is waited for. While it is the chat in front, the end is
//! held until the person moves away ([`front_moved`]). A blocked task and one the person
//! started from a tab are never ones to end. A task that had reported when the app quit is
//! left out of the next launch's put-back and is a finished row
//! ([`crate::finished::put_back_without_the_finished`]).
//!
//! What is left is its dispatch record, which its finished row is read from
//! ([`crate::finished`]), and the ledger's memory of how it ended, which answers a wait.
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
    self, Answered, Asked, Ends, Landed, Ledger, Looked, Reply, Row, Seen, Step, Waited, What,
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
    /// The finished tasks being reopened, by their dispatch record's id ([`Tasks::reopen`]).
    reopening: Mutex<HashMap<String, Reopening>>,
    /// Why the last Reopen of a finished task did not hold, by its record's id, for its row.
    not_reopened: Mutex<HashMap<String, String>>,
    /// Whether a reported task's end runs on the clock in a test ([`Tasks::on_the_clock`]).
    #[cfg(test)]
    clocked: std::sync::atomic::AtomicBool,
}

/// One finished task being reopened: the chat that was started for it, once one was, and when.
#[derive(Debug, Clone, Copy)]
struct Reopening {
    /// The new chat's number; none while it is being started.
    session: Option<u32>,
    started: Instant,
}

/// How long a reopened chat that purlis never heard from must have lived for its resume to
/// count as having worked. One that ends sooner is a harness that could not bring the
/// conversation back, and the task's row is put back.
pub const A_RESUME_HOLDS_AFTER: Duration = Duration::from_secs(15);

/// What a row says when the chat a Reopen started ended at once.
pub const NOT_RESUMED: &str = "It could not be reopened: its harness ended at once, without \
    bringing the conversation back (it may have been removed). Its report is still here.";

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

    fn reopens(&self) -> MutexGuard<'_, HashMap<String, Reopening>> {
        self.reopening
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// **Takes the one Reopen of finished task `id`** (#1485): `false` where one is already
    /// under way, so two presses start one chat. Tested and taken under one lock. Let go by
    /// [`Self::reopen_failed`], or settled when the chat it started is heard from or ends.
    pub(crate) fn reopen_begins(&self, id: &str) -> bool {
        let mut reopening = self.reopens();
        if reopening.contains_key(id) {
            return false;
        }
        reopening.insert(
            id.to_owned(),
            Reopening {
                session: None,
                started: Instant::now(),
            },
        );
        self.not_reopened
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        true
    }

    /// Whether any Reopen is under way: asked first, so a chat's move costs nothing more.
    fn any_reopening(&self) -> bool {
        !self.reopens().is_empty()
    }

    /// The Reopen of `id` started chat `session`.
    pub(crate) fn reopen_started(&self, id: &str, session: u32) {
        if let Some(entry) = self.reopens().get_mut(id) {
            entry.session = Some(session);
            entry.started = Instant::now();
        }
    }

    /// The Reopen of `id` started nothing: the row is as it was.
    pub(crate) fn reopen_failed(&self, id: &str) {
        self.reopens().remove(id);
    }

    /// Whether a Reopen of `id` is under way: its row is not drawn while one is.
    pub(crate) fn reopening(&self, id: &str) -> bool {
        self.reopens().contains_key(id)
    }

    /// Why the last Reopen of `id` did not hold, where it did not.
    pub(crate) fn not_reopened(&self, id: &str) -> Option<String> {
        self.not_reopened
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    /// Chat `session` was heard from (`ended` false) or has gone (`ended` true): where a
    /// Reopen started it, how that Reopen came out. `Some((id, true))` where the resume
    /// worked, and the task's row is to be cleared; `Some((id, false))` where the chat ended
    /// at once unheard, and the row stays with the reason.
    fn reopen_settles(&self, session: u32, ended: bool) -> Option<(String, bool)> {
        let mut reopening = self.reopens();
        let (id, entry) = reopening
            .iter()
            .find(|(_, entry)| entry.session == Some(session))
            .map(|(id, entry)| (id.clone(), *entry))?;
        reopening.remove(&id);
        drop(reopening);
        let worked = !ended || entry.started.elapsed() >= A_RESUME_HOLDS_AFTER;
        if !worked {
            self.not_reopened
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id.clone(), NOT_RESUMED.to_owned());
        }
        Some((id, worked))
    }

    /// In a test, a reported task's end waits on no clock unless the test says so here: the
    /// test looks for it ([`end_look`]) when it means the time to have passed. The hundreds of
    /// tests that go on using a task's chat after its report are then not in a race with a
    /// thread that would close it two seconds later.
    #[cfg(test)]
    pub(crate) fn on_the_clock(&self) {
        self.clocked
            .store(true, std::sync::atomic::Ordering::Relaxed);
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
        // A permission ask its hook holds open in the window's needs-you list (HP-6): a prompt
        // in front of the person whether or not the harness said it asked.
        ask_open: held.asks_open_for(chat),
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

/// `why`, the refusal for a chat that is not the asker's own open task; or, where `of` is a
/// task of its own that has finished and whose program purlis ended (#1485), the sentence that
/// says so: `asked` is what it wanted of it. Only of its own: the memory asked is keyed by the
/// asking chat, so the refusal still says nothing of any other chat.
fn or_finished(held: &Held, asker: u32, of: u32, why: String, asked: &str) -> String {
    match held.tasks().ledger().gone(asker, of) {
        Some(gone) => dispatched::finished_already(&gone.name, of, gone.report.as_ref(), asked),
        None => why,
    }
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
    let (from, name) = owned(held, asker, to)
        .map_err(|why| or_finished(held, asker, to, why, "no turn left to read a message in"))?;
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
        // Its row says whom it is asking from now (#1484): no hook reports a question.
        held.rows_changed();
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
    let (from, name) = owned(held, asker, to)
        .map_err(|why| or_finished(held, asker, to, why, "no question left to answer"))?;
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
    // Answered: its row stops saying it is asking (#1484).
    held.rows_changed();
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
    let (took_over, stepped_in) = {
        let mut ledger = held.tasks().ledger();
        (ledger.person_keyed(chat), ledger.stepped_in(chat))
    };
    // **A key of theirs in a task that had reported is the person taking it over** (#1485):
    // its end stood down, for good, and its record says so for the next launch.
    if took_over {
        crate::finished::taken_over(held, chat);
    }
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

/// The tasks under chat `asker`: the ones still open, in the order they were started, then
/// the ones that have finished and are still listed.
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
    let mut rows: Vec<Row> = crate::handoff::persona_chats(held, asker)
        .into_iter()
        .filter_map(|listed| {
            let open = open.iter().find(|open| open.session == listed.session)?;
            let from = open.from.as_ref()?;
            // Where it works on a branch of its own, by the dispatch's record (#1453). Read
            // before the ledger is taken: it asks the chats, and reads a file.
            let branch = crate::dispatches::branch_listed(held, listed.session);
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
                branch: branch.as_ref().map(|(branch, _)| branch.clone()),
                branch_stands: branch.map(|(_, stands)| stands),
                finished: false,
            })
        })
        .collect();
    // Then the ones that have finished, whose programs purlis ended (#1485): from their
    // dispatch records, until their rows are cleared or this chat closes.
    rows.extend(crate::finished::listed_for(held, asker));
    rows
}

/// Cancels task `of` for the chat that dispatched it: the cancel is recorded, and its turn is
/// ended and it is asked for a short report as soon as purlis may send its pane anything.
fn cancel(held: &Held, asker: u32, of: u32) -> Result<Answer, String> {
    let (began, name) = {
        // Under the lock a report is taken under, so a cancel and a report in flight land in
        // one order: either the task has reported and the cancel is refused, or the cancel is
        // recorded first and the report is delivered as cancelled.
        let _deciding = held.chats().deciding();
        let (from, name) = owned(held, asker, of)
            .map_err(|why| or_finished(held, asker, of, why, "nothing to cancel"))?;
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

/// Whether task `task` has a question open with the chat that dispatched it: asked, and not
/// answered yet. What its row in the window says it is waiting on (#1484).
pub fn asks_its_asker(held: &Held, task: u32) -> bool {
    held.tasks().ledger().talk.asks(task).is_some()
}

/// **Task `task`'s report is delivered: its program is now to be ended** (#1485). Called once
/// the lock the report was taken under is let go, by whoever took it. It ends nothing itself:
/// it looks once, for a task whose turn is already over, and sets the bound on how long the
/// rest may take ([`dispatched::ends_within`]).
pub fn delivered(held: &Held, task: u32) {
    if !held.tasks().ledger().ending(task) {
        return;
    }
    end_look_after(
        held,
        task,
        dispatched::ends_within(seen(held, task)),
        Looked::WaitedOut,
    );
    end_look(held, task, Looked::Moved);
}

/// **The end of task `task` stands down, for good**: a Smart close of it is beginning, which is
/// a turn of its own that the pending end must not cut. As for a key of the person's
/// ([`person_typed`]). Nothing where it was not one to end.
pub fn stand_down(held: &Held, task: u32) {
    if held.tasks().ledger().end_stood_down(task) {
        crate::finished::taken_over(held, task);
    }
}

/// The chat the person is looking at changed (another chat was brought in front, or a tab was
/// switched to another chat of its session): every task whose end was held is looked at
/// again, since one of them may have been held because the person had it in front of them.
pub fn front_moved(held: &Held) {
    let held_back = held.tasks().ledger().held_back();
    for task in held_back {
        end_look(held, task, Looked::Moved);
    }
}

/// Looks at task `task`, which has reported, and does the next thing toward ending its
/// program (`Ledger::end_step`). `looked` says why it is looked at.
///
/// **The end is only ever made here on a thread this module started**
/// ([`end_look_after`]): a look for [`Looked::Moved`] answers at most "settle", so the thread
/// that heard a hook or a program's end never closes a chat.
pub(crate) fn end_look(held: &Held, task: u32, looked: Looked) {
    end_look_reading(held, task, looked, &is_held_back);
}

/// [`end_look`], reading whether the task is held back with `held_back`: [`is_held_back`] in
/// the app, and a read a test can make change between the look's two reads of it.
pub(crate) fn end_look_reading(
    held: &Held,
    task: u32,
    looked: Looked,
    held_back: &dyn Fn(&Held, u32) -> bool,
) {
    if !held.tasks().ledger().ending(task) {
        return;
    }
    // The person's stop ends it, and says so in its own word; and a quit ends nothing here:
    // the chats it leaves are the ones the next launch reads.
    if held.stopping().is_stopping(task) || held.chats().ending() {
        return;
    }
    let seen = seen(held, task);
    // Read before the ledger is taken: it asks the chats and the board. **Held back** while a
    // chat it started is at work, and while it is the chat the person is looking at: the chat
    // in front, or the task its session's tab shows (`Chats::looked_at`, #1486). It is not
    // ended under them, and is ended when they move away from it ([`front_moved`]).
    let read = held_back;
    let held_back = read(held, task);
    let step = held
        .tasks()
        .ledger()
        .end_step(task, seen, held_back, looked);
    match step {
        Ends::Nothing => {}
        // Held, and the person may have looked away between the read above and the answer:
        // the look their move made found nothing held yet. Read once more, and look as that
        // move would have.
        Ends::Hold => {
            if held_back && held.tasks().ledger().held_back().contains(&task) && !read(held, task) {
                end_look(held, task, Looked::Moved);
            }
        }
        // What held it has gone and its bound passed meanwhile: the bound is set again.
        Ends::Bound => end_look_after(held, task, dispatched::ends_within(seen), Looked::WaitedOut),
        Ends::Settle => end_look_after(
            held,
            task,
            dispatched::A_TURN_SETTLES_WITHIN,
            Looked::Settled,
        ),
        Ends::End => end_it(held, task),
    }
}

/// Whether something that is not task `task`'s own turn holds its end back: a chat it started
/// is at work, or it is the chat the person is looking at.
fn is_held_back(held: &Held, task: u32) -> bool {
    held.chats().looked_at() == Some(task) || !crate::handoff::running_below(held, task).is_empty()
}

/// Ends task `task`'s program, its report delivered: the chat is closed as its tab's Close
/// closes it, under the lock a report is taken under, and the window takes its tab away where
/// it had one. Its finished row is its dispatch record's ([`crate::finished`]).
fn end_it(held: &Held, task: u32) {
    if let Err(why) = held.close_chat(task) {
        tracing::warn!("purlis: task chat {task}, reported, did not end cleanly ({why})");
    }
    held.tell_stop(task, crate::stopping::StopPhase::Stopped);
}

/// Looks at task `task`'s end again in `after`, as `looked`. On a thread of its own, holding
/// the project weakly: a closed project is not kept open by it.
fn end_look_after(held: &Held, task: u32, after: Duration, looked: Looked) {
    #[cfg(test)]
    if !held
        .tasks()
        .clocked
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return;
    }
    let held = held.weak();
    let _ = std::thread::Builder::new()
        .name("purlis-task-end".into())
        .spawn(move || {
            std::thread::sleep(after);
            if let Some(held) = held.upgrade() {
                end_look(&held, task, looked);
            }
        });
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
    // The line starts a turn: a task that had reported is working again from now (#1485).
    held.tasks().ledger().line_typed(chat);
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
    // A chat a Reopen started was heard from: its harness brought the conversation back.
    reopen_settled(held, report.chat, false);
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
        // A task that has reported: its turn's end is when its program is ended (#1485).
        end_look(held, chat, Looked::Moved);
    }
    // And the chat that started this one may be a reported task held back while this one was
    // at work: a turn ending here, as a close does, is when that hold may have gone.
    if let Some(from) = held.chats().handed_from(chat) {
        end_look(held, from.chat, Looked::Moved);
    }
    // A chat a Reopen started that has ended: how that Reopen came out.
    if held.tasks().any_reopening() && seen(held, chat).ended {
        reopen_settled(held, chat, true);
    }
    held.tasks().changed();
}

/// Chat `session`, which a Reopen of a finished task may have started, was heard from or has
/// gone: the task's row is cleared where the resume worked, and stays, saying why, where the
/// chat ended at once ([`Tasks::reopen_settles`]).
fn reopen_settled(held: &Held, session: u32, ended: bool) {
    if let Some((id, true)) = held.tasks().reopen_settles(session, ended)
        && let Err(why) = purlis_core::dispatchrecord::clear(held.root(), &id)
    {
        tracing::warn!("purlis: a reopened task's row was not cleared ({why})");
    }
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
    reopen_settled(held, chat, true);
    // The chat that asked for it may have been waiting for it to go before its own program
    // is ended (#1485). A look only: called under a close's lock, it ends nothing here.
    if let Some((asker, _)) = task_of {
        end_look(held, asker, Looked::Moved);
    }
    held.tasks().changed();
}

/// Chat `old` is now `new`: the same chat, started again under a new number. What the app
/// remembers of it, and the messages waiting for its next turn, follow it.
pub fn followed(held: &Held, old: u32, new: u32) {
    held.tasks().ledger().followed(old, new);
    dispatchtalk::moved(held.root(), old, new);
    // A task that had reported is still to be ended, under its new number (#1485).
    delivered(held, new);
    held.tasks().changed();
}
