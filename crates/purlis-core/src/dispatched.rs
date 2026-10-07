//! The tasks a chat dispatched, as that chat may ask after them (#1434, #1441): wait for one's
//! report, list them, and cancel one.
//!
//! A dispatch ([`crate::dispatchdecision`]) starts a persona chat that owes its asking chat one
//! report. This module is what the asking chat may do in the meantime, and the rules the app
//! answers it by. It is plain Rust with no clock, socket or terminal of its own: the app feeds
//! it its own records and what its board says, and does what it is told.
//!
//! # Whose task it is
//!
//! **Every ask here names a chat, and the app's record of that chat decides** ([`owned`]). A
//! chat may wait on, read, list and cancel only the tasks it dispatched itself: the record the
//! app wrote when it started the persona chat names the asking chat
//! ([`crate::reopen::HandedFrom`]), and nothing in a request can stand in for it. A sibling,
//! the chat above, a chat a handoff opened, a chat that is not open and a number no chat ever
//! had are all refused in one sentence, so a refusal says nothing about which chats exist.
//!
//! # Waiting
//!
//! `purlis dispatch --wait` and `purlis dispatch wait <chat>` are answered when the report
//! lands, or when the wait has gone on as long as it may ([`WAITS_AT_MOST`]), whichever comes
//! first ([`Ledger::waited`]). The report is kept for the asking chat's next turn all the same,
//! until the waiting command says it has it ([`What::Read`]): a command killed while it waits
//! loses nothing.
//!
//! # Being told
//!
//! A report nobody waited on is left for the asking chat's next turn, as it always was. Where
//! that chat is waiting for the person and asking nothing, the app also types one line of its
//! own into it ([`nudge`]), which starts that turn. What is typed is purlis's sentence and a
//! chat's number; the report itself arrives as quoted data, never as typed text. One line per
//! batch of reports ([`Ledger::nudge_step`]), so a harness that hands a turn no context is
//! typed into once and not in a loop.
//!
//! # Cancelling
//!
//! Cancel ends the persona chat's turn and asks it for a short report ([`Ledger::cancel_step`]).
//! Nothing is typed into a chat that is showing the person a prompt: the cancel waits for the
//! answer. Whatever the chat then says of its outcome, the report is delivered as `cancelled`,
//! because that is the app's own record of what happened; and a chat that ends its turn, or
//! ends altogether, without one has a report written for it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::dispatchdecision::Mode;
use crate::handback::{Handback, Outcome};
use crate::reopen::{HandedFrom, Owed};

// ---- on the wire ------------------------------------------------------------------------------

/// What a chat asks the app about the tasks it dispatched, as `purlis dispatch wait`, `list` and
/// `cancel` hand it over.
///
/// **It says nothing about the chat that asks but its number**, as a dispatch does
/// ([`crate::hookwire::DispatchAsk`]): the line must carry that chat's token, from inside that
/// chat (D-1407-6/10), and what the chat may do with the task it names is the app's record of
/// both. No ticket: nothing here starts a chat, and each ask is judged whole every time it is
/// made.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Asked {
    /// The chat that asks, from [`crate::hookwire::CHAT_ENV`].
    pub chat: u32,
    pub what: What,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum What {
    /// Wait for task `of`'s report, for at most `within_secs` ([`wait_secs`]).
    Wait { of: u32, within_secs: u32 },
    /// The waiting command has task `of`'s report: it need not be handed to the next turn too.
    Read { of: u32 },
    /// The tasks this chat dispatched.
    List,
    /// Cancel task `of`.
    Cancel { of: u32 },
    /// Send task `to` a follow-up (#1442). Refused unless this chat dispatched it and it is
    /// still working.
    Tell { to: u32, text: String },
    /// Send this task's asking chat a progress note. It names no recipient: the chat it goes
    /// to is the one the app recorded as this chat's asker.
    Note { text: String },
    /// Ask this task's asking chat a question, which this chat pauses on. It names no
    /// recipient, as a note names none.
    Question { text: String },
    /// Answer the question task `to` asked this chat.
    Answer { to: u32, text: String },
    /// Wait for the answer to this task's question, for at most `within_secs`.
    AwaitAnswer { within_secs: u32 },
    /// The waiting command has the answer: it need not be handed to the next turn too.
    GotAnswer,
}

/// What the app answers an [`Asked`] with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answered {
    /// A wait ended: on task `of`, called `name`.
    Waited { of: u32, name: String, what: Waited },
    /// Taken note of.
    Noted,
    /// The asking chat's tasks, oldest first.
    Listed { rows: Vec<Row> },
    /// Task `of`, called `name`, is being cancelled; its report follows as any report does.
    Cancelling { of: u32, name: String },
    /// A message was left for the chat called `to`, for its next turn (#1442).
    Sent {
        kind: crate::dispatchtalk::Kind,
        to: String,
    },
    /// A wait for an answer ended.
    Replied { what: Reply },
}

/// How a task's wait for the answer to its question ended.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    /// The chat called `from` answered.
    Answered { from: String, text: String },
    /// The wait went on as long as it may, and the chat called `from` has not answered.
    NotYet { from: String },
    /// The chat that asked for the task has closed: nobody is left to answer.
    AskerGone,
}

/// How a wait ended.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Waited {
    /// The report, as the asking chat's turn would have been handed it.
    Reported { report: Box<Handback> },
    /// The wait went on as long as it may, and the task has not reported: where it stands.
    Running { state: String },
    /// It reported earlier, and that report was already handed to this chat.
    AlreadyRead,
    /// Its program ended without a report.
    Ended,
    /// The task asks this chat a question and is paused on it (#1442). A wait is answered with
    /// a question once; the wait after the answer waits on for the report.
    Asks { question: String },
}

/// One task in a chat's list of the tasks it dispatched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Row {
    /// The app's number for the persona chat: what `wait` and `cancel` take.
    pub chat: u32,
    /// The task's name.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// Where it works, by [`crate::active::Place::word`].
    pub place: String,
    /// [`State::say`].
    pub state: String,
    /// How long ago it was started, where the app saw it start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_secs: Option<u64>,
    /// The person started it from this chat's tab (#1438): its report comes to this chat, and
    /// this chat can wait on, tell, answer and cancel nothing of it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_person: bool,
}

// ---- how long a wait may be -------------------------------------------------------------------

/// How long `--wait` waits when it is not told: under the two minutes a harness gives a
/// command by default, with room for the answer to be read.
pub const WAITS_BY_DEFAULT: u32 = 100;

/// The longest a wait is held, whatever it asks: under the ten minutes a harness gives a
/// command at most.
pub const WAITS_AT_MOST: u32 = 540;

/// How long a wait that asked for `asked` seconds is held.
pub fn wait_secs(asked: u32) -> u32 {
    asked.clamp(1, WAITS_AT_MOST)
}

// ---- whose task it is -------------------------------------------------------------------------

/// What every ask about a chat that is not the asker's own task is told. One sentence for
/// every reason, so a refusal does not say which chats exist or what they are.
pub fn not_yours(of: u32) -> String {
    format!(
        "chat {of} is not a task this chat dispatched, so this chat has nothing to do with it. \
         `purlis dispatch list` shows the tasks it did dispatch."
    )
}

/// The app's record of chat `of`'s dispatch, where `asker` dispatched it as a task; else the
/// refusal. `record` is what the app holds for `of`, or none for a chat it does not have open.
pub fn owned(asker: u32, of: u32, record: Option<&HandedFrom>) -> Result<&HandedFrom, String> {
    record
        .filter(|from| is_owner(asker, of, from))
        .ok_or_else(|| not_yours(of))
}

/// **The one rule of who owns a task**: the chat its record names as its asking chat, where a
/// dispatch started it as a task. Every power over a task (wait, read, cancel, tell, answer)
/// is this, so a rule added here holds for all of them.
///
/// **A task the person started from a chat's tab gives that chat none** (#1438, D-T59-j1). Its
/// record names the tab's chat, because that is where its report goes; but that chat asked for
/// nothing and holds no grant for the pair, and the person consented to one question, not to
/// that chat steering the persona. It reads the report, sees the task listed, and that is all.
fn is_owner(asker: u32, of: u32, from: &HandedFrom) -> bool {
    from.chat == asker && from.mode == Mode::Task && asker != of && !from.by_person
}

/// What a chat is told when it cancels a task the person is already stopping (D-T59-j3): the
/// stop is the person's and the later word, and it ends in the chat being told all the same.
pub fn being_stopped(name: &str, of: u32) -> String {
    format!(
        "'{name}' (chat {of}) is being stopped by the operator, so there is nothing to cancel. \
         This chat is told when it has ended."
    )
}

// ---- where a task stands ----------------------------------------------------------------------

/// A chat as the app has it at this moment: what its board says, and what its harness is.
///
/// **The default is a chat nothing is known of**, and nothing is ever typed into one: not
/// heard from, not measured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Seen {
    /// Its program has ended, or the chat is not open.
    pub ended: bool,
    /// Its harness has reported to the board since the chat started. Until it has, purlis
    /// does not know what the program is showing: a start-up dialog, a login, anything.
    pub heard: bool,
    /// A turn purlis heard begin is under way, and it has shown the person no prompt.
    pub running: bool,
    /// Its turn has ended and it is waiting to be prompted.
    pub waiting: bool,
    /// It has shown the person a prompt this turn: a permission, or a question. The board
    /// keeps this until the turn ends, not until the prompt is answered.
    pub asking: bool,
    /// Its harness is one purlis has measured its typed line in ([`told_by_a_line`]).
    pub measured: bool,
}

impl Seen {
    /// Whether purlis knows enough of this chat to send its pane any key at all: a measured
    /// harness that has been heard from, still running its program, showing no prompt.
    fn takes_keys(self) -> bool {
        self.measured && self.heard && !self.ended && !self.asking
    }

    /// Whether a line may be typed into it: [`Self::takes_keys`], and waiting for a prompt.
    /// The person's own keys in its pane are the ledger's to know ([`Ledger::person_keyed`]).
    pub fn takes_a_line(self) -> bool {
        self.takes_keys() && self.waiting
    }

    /// Whether its turn may be interrupted: [`Self::takes_keys`], in a turn purlis heard begin.
    pub fn takes_an_interrupt(self) -> bool {
        self.takes_keys() && self.running
    }

    /// Its turn has ended, by its harness's own word, whatever its harness is.
    fn turn_ended(self) -> bool {
        self.heard && !self.ended && self.waiting && !self.asking
    }
}

/// Where a task stands, as its asking chat is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    /// Its turn ended and it has not reported.
    Idle,
    /// It is waiting on the person, in its own tab.
    NeedsThePerson,
    /// It asked its asking chat a question, and is paused until that chat answers.
    AsksYou,
    Cancelling,
    /// It reported; how, where the app still knows.
    Reported(Option<Outcome>),
    /// Its program ended without a report.
    Ended,
}

impl State {
    pub fn say(self) -> String {
        match self {
            Self::Running => "running".to_owned(),
            Self::Idle => "idle, with no report yet".to_owned(),
            // One word for it, whoever lists the task (#1443).
            Self::NeedsThePerson => crate::dispatchdecision::Standing::WaitingOnOperator
                .word()
                .to_owned(),
            Self::AsksYou => "asking this chat a question".to_owned(),
            Self::Cancelling => "cancelling".to_owned(),
            Self::Reported(Some(outcome)) => format!("reported: {}", outcome.word()),
            Self::Reported(None) => "reported".to_owned(),
            Self::Ended => "ended without a report".to_owned(),
        }
    }
}

// ---- what the app remembers -------------------------------------------------------------------

/// What the app remembers of the tasks dispatched in one project, beyond the lineage each
/// chat's own record holds: when it saw each start, the report each sent, and how far a cancel
/// has got. **In memory**, and gone with the app: after a relaunch a task is still its asking
/// chat's, by its record, and its age and its report's text are not known.
#[derive(Debug, Default)]
pub struct Ledger {
    tasks: HashMap<u32, Task>,
    /// By chat: what was left for its next turn that it has not been told of.
    landed: HashMap<u32, Vec<Landed>>,
    /// The messages between asking chats and their tasks (#1442).
    pub talk: crate::dispatchtalk::Talk,
    /// The chats whose pane a key of the person's has gone to since the chat's last
    /// `UserPromptSubmit` or `Stop`. They may have a harness's own picker open, which no hook
    /// reports, so nothing is typed into one until its harness says a turn began or ended.
    keyed: std::collections::HashSet<u32>,
    /// Tasks whose chats have closed, oldest first: who asked, what it was called, and its
    /// report where it sent one. So a wait on one says how it ended.
    gone: std::collections::VecDeque<(u32, Gone)>,
}

/// A task whose chat has closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gone {
    pub asker: u32,
    pub name: String,
    pub report: Option<Handback>,
}

/// Something left for a chat's next turn, which it is typed a line about when it may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    /// The report of this task, which the chat dispatched.
    Report(u32),
    /// A question from this task, which the chat dispatched.
    Question(u32),
    /// A follow-up from the chat that dispatched this one.
    FollowUp,
    /// The answer to this chat's question, from the chat that dispatched it.
    Answer,
}

#[derive(Debug, Default)]
struct Task {
    started: Option<SystemTime>,
    /// Its asking chat cancelled it. Kept once the cancel is over: it is the report's outcome.
    cancelled: bool,
    /// The person typed in its pane while it worked (#1442). The fact alone: nothing they
    /// typed is kept anywhere.
    stepped_in: bool,
    /// How far the cancel has got, while one is under way.
    cancel: Option<Cancel>,
    report: Option<Kept>,
}

/// A report the app delivered, kept so a wait can answer with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub report: Handback,
    /// The file it waits in for the asking chat's next turn, until a wait has read it.
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cancel {
    /// Asked for; nothing has been sent to the chat.
    Asked,
    /// Its turn was interrupted.
    Interrupted,
    /// Its interrupted turn has had its moment to stop ([`A_TURN_STOPS_WITHIN`]). Remembered,
    /// so a line held back then is typed when the hold lifts.
    Settled,
    /// It was typed the line asking for a short report.
    Prompted,
    /// Its harness said that line began a turn.
    Heard,
}

/// What the app does next for a cancel ([`Ledger::cancel_step`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing is under way.
    Nothing,
    /// Not now: purlis has not heard from the chat, its harness is not one it types into, it
    /// has shown the person a prompt this turn, the person has pressed a key in it, or the
    /// line it was sent has not been answered yet. Asked again when the chat next moves.
    Hold,
    /// End the chat's turn ([`INTERRUPT`]), and ask again once it has had a moment to stop
    /// ([`A_TURN_STOPS_WITHIN`]).
    Interrupt,
    /// Type [`CANCEL_PROMPT`] into it, and ask again if no turn is heard to begin on it
    /// ([`A_PROMPT_IS_HEARD_WITHIN`], [`Ledger::prompt_unheard`]).
    Prompt,
    /// It will send no report: write this one for it, as `cancelled`.
    Write(&'static str),
}

/// The byte that ends a harness's turn: Escape, as the person presses it.
pub const INTERRUPT: &[u8] = b"\x1b";

/// How long an interrupted turn is given to stop before the chat is asked for its report. No
/// harness reports an interrupted turn, so the app cannot hear that it stopped.
pub const A_TURN_STOPS_WITHIN: Duration = Duration::from_millis(1500);

/// What a cancelled persona chat is typed. purlis's own sentence: nothing a chat wrote is in it.
pub const CANCEL_PROMPT: &str = "purlis: the chat that asked for this task has cancelled it. \
    Stop the work now, and send one short report of what you did and what you left unfinished: \
    purlis dispatch report --outcome cancelled \"<a few lines>\"";

/// The report written for a cancelled chat whose program had ended.
pub const ENDED_UNREPORTED: &str =
    "The task was cancelled by the chat that asked for it. Its chat had ended and sent no report.";

/// The report written for a cancelled chat that ended its turn without sending one.
pub const CANCELLED_UNREPORTED: &str = "The task was cancelled by the chat that asked for it. Its \
    chat was asked for a short report and ended its turn without sending one.";

/// The report written for a cancelled chat purlis types nothing into, when its turn ends.
pub const CANCELLED_UNASKED: &str = "The task was cancelled by the chat that asked for it. Its \
    chat runs a harness purlis does not type into, so it was not asked for a report; its turn \
    has ended.";

/// How long after the cancel line is typed a turn may take to be heard to begin on it. Past
/// it, the line counts as taken: the chat's next idle moment with no report ends the cancel.
pub const A_PROMPT_IS_HEARD_WITHIN: Duration = Duration::from_secs(60);

/// The most closed tasks remembered, so a wait on one says how it ended.
const MOST_GONE: usize = 64;

/// What an outcome of `cancelled` from a chat nobody cancelled is told.
pub const NOT_CANCELLED: &str = "this task was not cancelled, and only a task its asking chat \
    cancelled reports that outcome. Use done, blocked or failed";

/// The line typed into a chat that is waiting, to say what was left for the turn it starts: the
/// reports and questions of tasks it dispatched, and a message from the chat that dispatched
/// it. purlis's own sentence and the app's numbers for the chats: nothing a chat wrote is in it.
pub fn nudge(landed: &[Landed]) -> String {
    let chats = |pick: fn(&Landed) -> Option<u32>| -> Vec<String> {
        landed
            .iter()
            .filter_map(pick)
            .map(|chat| chat.to_string())
            .collect()
    };
    let reports = chats(|one| match one {
        Landed::Report(chat) => Some(*chat),
        _ => None,
    });
    let questions = chats(|one| match one {
        Landed::Question(chat) => Some(*chat),
        _ => None,
    });
    let from_above = landed
        .iter()
        .filter(|one| matches!(one, Landed::FollowUp | Landed::Answer))
        .count();
    let mut said = Vec::new();
    match reports.as_slice() {
        [] => {}
        [one] => said.push(format!(
            "a task this chat dispatched has reported (chat {one})"
        )),
        many => said.push(format!(
            "tasks this chat dispatched have reported (chats {})",
            many.join(", ")
        )),
    }
    match questions.as_slice() {
        [] => {}
        [one] => said.push(format!(
            "a task this chat dispatched asks it a question (chat {one})"
        )),
        many => said.push(format!(
            "tasks this chat dispatched ask it questions (chats {})",
            many.join(", ")
        )),
    }
    match from_above {
        0 => {}
        1 => said.push("the chat that asked for this task has sent it a message".to_owned()),
        _ => said.push("the chat that asked for this task has sent it messages".to_owned()),
    }
    let one = reports.len() + questions.len() + from_above == 1;
    let attached = if one {
        "It is attached to this turn as context, quoted as data."
    } else {
        "They are attached to this turn as context, quoted as data."
    };
    // What a turn handed nothing can run instead: only a task's report or question can be
    // asked for again, by the chat that dispatched it.
    let mut asked_for: Vec<&String> = reports.iter().chain(&questions).collect();
    asked_for.dedup();
    let otherwise = match asked_for.as_slice() {
        [] => String::new(),
        [chat] if one => format!(" If it is not there, run `purlis dispatch wait {chat}`."),
        _ => " If one is not there, run `purlis dispatch wait <chat>` for it.".to_owned(),
    };
    format!("purlis: {}. {attached}{otherwise}", said.join(", and "))
}

/// Whether `harness` is one purlis types a line of its own into (D-1441-13): the harnesses
/// purlis arms a `UserPromptSubmit` hook on and has measured to hand that turn its context,
/// and to end a turn on Escape. Codex counts only once its hooks are trusted, which is what
/// [`Seen::heard`] says: an untrusted Codex reports nothing. On any other harness, and in a
/// chat with none, nothing is typed: a report or a message waits for the next turn the person
/// starts, and a cancel takes effect when the turn ends.
pub fn told_by_a_line(harness: Option<crate::harness::Harness>) -> bool {
    use crate::harness::Harness;
    matches!(harness, Some(Harness::ClaudeCode | Harness::Codex))
}

impl Ledger {
    /// The app started the persona chat `task` at `now`.
    pub fn started(&mut self, task: u32, now: SystemTime) {
        self.tasks.entry(task).or_default().started = Some(now);
    }

    /// Chat `chat` has closed. `task_of` is its asking chat and its name, where it was a task:
    /// that much is kept, with its report, so a wait on it says how it ended. Nothing else is
    /// remembered of it, as a task or as an asking chat.
    pub fn forget(&mut self, chat: u32, task_of: Option<(u32, &str)>) {
        let entry = self.tasks.remove(&chat);
        if let Some((asker, name)) = task_of {
            if self.gone.len() >= MOST_GONE {
                self.gone.pop_front();
            }
            self.gone.push_back((
                chat,
                Gone {
                    asker,
                    name: name.to_owned(),
                    report: entry.and_then(|entry| entry.report).map(|kept| kept.report),
                },
            ));
        }
        self.landed.remove(&chat);
        self.keyed.remove(&chat);
        for told in self.landed.values_mut() {
            told.retain(
                |one| !matches!(one, Landed::Report(of) | Landed::Question(of) if *of == chat),
            );
        }
        self.gone.retain(|(_, gone)| gone.asker != chat);
        self.talk.forget(chat);
    }

    /// The closed task `task`, where `asker` dispatched it.
    pub fn gone(&self, asker: u32, task: u32) -> Option<&Gone> {
        self.gone
            .iter()
            .find(|(of, gone)| *of == task && gone.asker == asker)
            .map(|(_, gone)| gone)
    }

    /// Chat `old` is now `new`: the same chat, started again under a new number. Everything
    /// remembered of it follows it, as a task and as an asking chat.
    pub fn followed(&mut self, old: u32, new: u32) {
        if old == new {
            return;
        }
        if let Some(entry) = self.tasks.remove(&old) {
            self.tasks.insert(new, entry);
        }
        if let Some(told) = self.landed.remove(&old) {
            self.landed.insert(new, told);
        }
        for told in self.landed.values_mut() {
            for one in told.iter_mut() {
                match one {
                    Landed::Report(of) | Landed::Question(of) if *of == old => *of = new,
                    _ => {}
                }
            }
        }
        for (_, gone) in &mut self.gone {
            if gone.asker == old {
                gone.asker = new;
            }
        }
        // A fresh program: whatever the person keyed went to the old one.
        self.keyed.remove(&old);
        self.talk.followed(old, new);
    }

    /// A key of the person's went to chat `chat`'s pane.
    pub fn person_keyed(&mut self, chat: u32) {
        self.keyed.insert(chat);
    }

    /// Whether a key of the person's has gone to chat `chat`'s pane since its harness last
    /// said a turn began or ended.
    pub fn keyed(&self, chat: u32) -> bool {
        self.keyed.contains(&chat)
    }

    /// `what` was left for chat `chat`'s next turn.
    pub fn landed(&mut self, chat: u32, what: Landed) {
        let told = self.landed.entry(chat).or_default();
        if !told.contains(&what) {
            told.push(what);
        }
    }

    /// The person typed in task `task`'s pane while it worked.
    pub fn person_typed(&mut self, task: u32) {
        self.tasks.entry(task).or_default().stepped_in = true;
    }

    /// Whether the person typed in task `task`'s pane: what its report says, and all it says.
    pub fn stepped_in(&self, task: u32) -> bool {
        self.tasks.get(&task).is_some_and(|task| task.stepped_in)
    }

    /// Whether anything here waits for chat `chat` to move: a cancel of it under way, or
    /// something it has not been told of.
    pub fn waits_on(&self, chat: u32) -> bool {
        self.tasks
            .get(&chat)
            .is_some_and(|task| task.cancel.is_some())
            || self.landed.get(&chat).is_some_and(|told| !told.is_empty())
    }

    /// How long ago `task` started, where the app saw it start.
    pub fn age(&self, task: u32, now: SystemTime) -> Option<Duration> {
        let started = self.tasks.get(&task)?.started?;
        Some(now.duration_since(started).unwrap_or_default())
    }

    /// The outcome `task`'s report is delivered with, given the one its chat `said`: a
    /// cancelled task's is `cancelled` whatever it says, and no other task's can be.
    pub fn outcome_for(&self, task: u32, said: Outcome) -> Result<Outcome, String> {
        let cancelled = self.tasks.get(&task).is_some_and(|task| task.cancelled);
        match (cancelled, said) {
            (true, _) => Ok(Outcome::Cancelled),
            (false, Outcome::Cancelled) => Err(NOT_CANCELLED.to_owned()),
            (false, said) => Ok(said),
        }
    }

    /// `task`'s report was delivered: `report`, left in `file` for the asking chat `asker`'s
    /// next turn where that chat is open (`file` is none for one kept for a workspace). A
    /// cancel of it is over.
    pub fn reported(&mut self, task: u32, asker: u32, report: Handback, file: Option<PathBuf>) {
        if file.is_some() {
            self.landed(asker, Landed::Report(task));
        }
        // A question it had open is closed with it: an answer now would reach no turn of the
        // work, and the chat is typed nothing about one.
        self.talk.close(task);
        if let Some(told) = self.landed.get_mut(&task) {
            told.retain(|one| !matches!(one, Landed::FollowUp | Landed::Answer));
        }
        let entry = self.tasks.entry(task).or_default();
        entry.cancel = None;
        entry.report = Some(Kept { report, file });
    }

    /// The report `task` sent, where the app still has it.
    pub fn report(&self, task: u32) -> Option<&Kept> {
        self.tasks.get(&task)?.report.as_ref()
    }

    /// A waiting command has what a wait on `task` answered with, its report or its question:
    /// the files that were left for the asking chat's next turn, to remove, where they are
    /// still there. The asking chat is not told of them again.
    pub fn read(&mut self, task: u32) -> Vec<PathBuf> {
        let report = self
            .tasks
            .get_mut(&task)
            .and_then(|entry| entry.report.as_mut())
            .and_then(|kept| kept.file.take());
        let question = self.talk.read(task);
        for told in self.landed.values_mut() {
            told.retain(|one| match one {
                Landed::Report(of) => *of != task || report.is_none(),
                Landed::Question(of) => *of != task || question.is_none(),
                Landed::FollowUp | Landed::Answer => true,
            });
        }
        report.into_iter().chain(question).collect()
    }

    /// The waiting command of task `task` has the answer to its question: the file it was left
    /// in for the task's next turn, to remove. The task is not typed a line about it.
    pub fn got_answer(&mut self, task: u32) -> Option<PathBuf> {
        if let Some(told) = self.landed.get_mut(&task) {
            told.retain(|one| *one != Landed::Answer);
        }
        self.talk.got_answer(task)
    }

    /// Where `task` stands, by its own record `from` and what the board says of it.
    pub fn state(&self, task: u32, from: &HandedFrom, seen: Seen) -> State {
        let entry = self.tasks.get(&task);
        if from.report == Owed::Sent {
            let outcome = entry
                .and_then(|entry| entry.report.as_ref())
                .and_then(|kept| kept.report.task.as_ref())
                .map(|task| task.outcome);
            return State::Reported(outcome);
        }
        if seen.ended {
            State::Ended
        } else if entry.is_some_and(|entry| entry.cancel.is_some()) {
            State::Cancelling
        } else if self.talk.asks(task).is_some() {
            State::AsksYou
        } else if seen.asking {
            State::NeedsThePerson
        } else if seen.waiting {
            State::Idle
        } else {
            State::Running
        }
    }

    /// How a wait on `task` ends now, or none while there is still something to wait for.
    /// A question the task asked ends one wait, and is marked shown by it.
    pub fn waited(&mut self, task: u32, from: &HandedFrom, seen: Seen) -> Option<Waited> {
        if let Some(kept) = self.report(task) {
            return Some(Waited::Reported {
                report: Box::new(kept.report.clone()),
            });
        }
        if from.report == Owed::Sent {
            return Some(Waited::AlreadyRead);
        }
        if seen.ended {
            return Some(Waited::Ended);
        }
        self.talk
            .show(task)
            .map(|question| Waited::Asks { question })
    }

    /// The asking chat cancels `task`, called `name`, whose record is `from`. Answers whether a
    /// cancel began: `false` for one already under way, which is left as it is. Refused for a
    /// task that has reported, with how it stands.
    pub fn cancel(&mut self, task: u32, name: &str, from: &HandedFrom) -> Result<bool, String> {
        if from.report == Owed::Sent {
            let state = self.state(task, from, Seen::default());
            return Err(format!(
                "'{name}' (chat {task}) has already {}, so there is nothing to cancel.",
                match state {
                    State::Reported(Some(outcome)) => format!("reported ({})", outcome.word()),
                    _ => "reported".to_owned(),
                }
            ));
        }
        let entry = self.tasks.entry(task).or_default();
        if entry.cancel.is_some() {
            return Ok(false);
        }
        entry.cancelled = true;
        entry.cancel = Some(Cancel::Asked);
        Ok(true)
    }

    /// What the app does next for `task`'s cancel, and the cancel moved on by it.
    ///
    /// `owed` is what the task's record says it owes, `seen` what the app knows of its chat,
    /// and `settled` whether [`A_TURN_STOPS_WITHIN`] has just passed since its turn was
    /// interrupted.
    ///
    /// **A key goes to a chat's pane only where purlis knows what the pane is showing**
    /// (D-1441-13): its harness is one purlis has measured, the board has heard from it since
    /// it started, it has shown the person no prompt this turn, and no key of the person's has
    /// gone to it since its last report. Escape, further, only into a turn purlis heard
    /// begin. Anywhere else the step is [`Step::Hold`]: the cancel is recorded, the report is
    /// delivered as `cancelled` when it comes, and one is written when the chat's turn or
    /// program ends without it.
    pub fn cancel_step(&mut self, task: u32, owed: Owed, seen: Seen, settled: bool) -> Step {
        let keyed = self.keyed.contains(&task);
        let Some(entry) = self.tasks.get_mut(&task) else {
            return Step::Nothing;
        };
        let Some(mut stage) = entry.cancel else {
            return Step::Nothing;
        };
        if owed != Owed::Due {
            entry.cancel = None;
            return Step::Nothing;
        }
        if seen.ended {
            entry.cancel = None;
            return Step::Write(ENDED_UNREPORTED);
        }
        if stage == Cancel::Interrupted && settled {
            stage = Cancel::Settled;
            entry.cancel = Some(stage);
        }
        match stage {
            // The line was taken, and the turn it began has ended with no report.
            Cancel::Heard if seen.turn_ended() => {
                entry.cancel = None;
                Step::Write(CANCELLED_UNREPORTED)
            }
            Cancel::Heard | Cancel::Prompted => Step::Hold,
            // A harness purlis types nothing into: the cancel takes effect when its turn ends.
            Cancel::Asked | Cancel::Interrupted | Cancel::Settled if !seen.measured => {
                if seen.turn_ended() {
                    entry.cancel = None;
                    Step::Write(CANCELLED_UNASKED)
                } else {
                    Step::Hold
                }
            }
            Cancel::Asked | Cancel::Interrupted | Cancel::Settled => {
                if keyed {
                    Step::Hold
                } else if seen.takes_a_line() || (stage == Cancel::Settled && seen.takes_keys()) {
                    entry.cancel = Some(Cancel::Prompted);
                    Step::Prompt
                } else if stage == Cancel::Asked && seen.takes_an_interrupt() {
                    entry.cancel = Some(Cancel::Interrupted);
                    Step::Interrupt
                } else {
                    Step::Hold
                }
            }
        }
    }

    /// No turn was heard to begin on the cancel line typed into `task` within
    /// [`A_PROMPT_IS_HEARD_WITHIN`]: the line counts as taken, so the cancel ends at the chat's
    /// next idle moment with no report, and never stays "cancelling" for good.
    pub fn prompt_unheard(&mut self, task: u32) {
        if let Some(entry) = self.tasks.get_mut(&task)
            && entry.cancel == Some(Cancel::Prompted)
        {
            entry.cancel = Some(Cancel::Heard);
        }
    }

    /// Whether a cancel of `task` is under way.
    /// **The person is stopping `task`** (D-T59-j3): a cancel of it stands down, for good. The
    /// stop is the later word and the person's, so nothing more is sent for the cancel, and
    /// what the chat reports in its last turn is its own outcome, not `cancelled`.
    pub fn stood_down(&mut self, task: u32) {
        if let Some(entry) = self.tasks.get_mut(&task) {
            entry.cancel = None;
            entry.cancelled = false;
        }
    }

    pub fn cancelling(&self, task: u32) -> bool {
        self.tasks
            .get(&task)
            .is_some_and(|entry| entry.cancel.is_some())
    }

    /// Chat `chat`'s harness said a prompt began a turn. As a cancelled task, the line it was
    /// typed has been taken; as an asking chat, the turn was handed every report that waited.
    /// And whatever the person had open in its pane is behind it.
    pub fn turn_began(&mut self, chat: u32) {
        if let Some(entry) = self.tasks.get_mut(&chat)
            && entry.cancel == Some(Cancel::Prompted)
        {
            entry.cancel = Some(Cancel::Heard);
        }
        self.landed.remove(&chat);
        self.keyed.remove(&chat);
        self.talk.turn_began(chat);
    }

    /// Chat `chat`'s harness said its turn ended: whatever the person had open in its pane is
    /// behind it.
    pub fn turn_ended(&mut self, chat: u32) {
        self.keyed.remove(&chat);
    }

    /// What chat `chat` is typed a line about now, or nothing: something was left for its next
    /// turn that it was not told of, it takes a line ([`Seen::takes_a_line`]), and no key of
    /// the person's has gone to its pane since its last report. Taken, so one batch is one
    /// line.
    pub fn nudge_step(&mut self, chat: u32, seen: Seen) -> Vec<Landed> {
        if !seen.takes_a_line() || self.keyed.contains(&chat) {
            return Vec::new();
        }
        self.landed.remove(&chat).unwrap_or_default()
    }
}

/// **Whether the person's keys in a task's pane are them stepping in** (#1442), as opposed to
/// answering a prompt the task put to them.
///
/// Outside a turn that has shown them a prompt, any key is. Within one the board cannot say
/// when the prompt was answered, so the keys are read: picking an option is a key or two and
/// Enter, and an instruction is words. `line` is how many bytes they have typed since their
/// last Enter, where purlis could follow them, and `bytes` what they sent now. Two bytes of
/// text or more, typed or pasted, is stepping in.
pub fn steps_in(asked_this_turn: bool, line: Option<usize>, bytes: &[u8]) -> bool {
    if !asked_this_turn {
        return true;
    }
    let text = |bytes: &[u8]| {
        bytes
            .iter()
            .filter(|byte| **byte >= 0x20 && **byte != 0x7f)
            .count()
    };
    // A paste carries its text between markers, which are not the text.
    let pasted = bytes
        .strip_prefix(b"\x1b[200~")
        .map(|rest| rest.strip_suffix(b"\x1b[201~").unwrap_or(rest));
    let now = match pasted {
        Some(pasted) => text(pasted),
        // Any other escape sequence is a key (an arrow, a function key), not text.
        None if bytes.first() == Some(&0x1b) => 0,
        None => text(bytes),
    };
    now >= 2 || line.is_some_and(|line| line >= 2)
}

// ---- said to the chat -------------------------------------------------------------------------

/// A span of time in the two largest units it has: `45s`, `3m`, `2h 5m`, `3d 4h`.
pub fn age_word(age: Duration) -> String {
    let secs = age.as_secs();
    let (days, hours, minutes) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{secs}s")
    }
}

/// What a listed task the person started from this chat's tab says after its row.
pub const STARTED_BY_THE_PERSON: &str = " · started by the person from this chat's tab: its \
    report comes here, and it is not this chat's to wait on, tell or cancel";

/// A chat's list of the tasks it dispatched, as `purlis dispatch list` prints it: persona,
/// task, where, state and age, one task a line.
pub fn list_text(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "This chat has no dispatched tasks open. A task is listed from its dispatch until \
                its chat is closed."
            .to_owned();
    }
    // A task the person started from this chat's tab is listed, and said to be theirs.
    let theirs = rows.iter().filter(|row| row.by_person).count();
    let mut said = format!(
        "{} {}:",
        if rows.len() == 1 {
            "1 task".to_owned()
        } else {
            format!("{} tasks", rows.len())
        },
        if theirs == 0 {
            "dispatched by this chat"
        } else {
            "under this chat"
        }
    );
    for row in rows {
        let place = match crate::active::Place::read(&row.place) {
            Some(place) => place.said(),
            None => format!("'{}'", crate::personas::one_line(&row.place)),
        };
        let age = match row.age_secs {
            Some(secs) => format!("started {} ago", age_word(Duration::from_secs(secs))),
            None => "started before this app did".to_owned(),
        };
        said.push_str(&format!(
            "\n- chat {} · '{}' · {} · {place} · {} · {age}{}",
            row.chat,
            crate::personas::one_line(&row.name),
            match &row.persona {
                Some(persona) => crate::personas::one_line(persona),
                None => "no persona".to_owned(),
            },
            crate::personas::one_line(&row.state),
            if row.by_person {
                STARTED_BY_THE_PERSON
            } else {
                ""
            },
        ));
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active::Place;
    use crate::handback::Task as Reported;

    const ASKER: u32 = 3;
    const TASK: u32 = 9;

    fn dispatched_by(chat: u32) -> HandedFrom {
        HandedFrom {
            chat,
            name: "steward 3".to_owned(),
            workspace: Place::Workspace("alpha".to_owned()),
            report: Owed::Due,
            mode: Mode::Task,
            depth: 1,
            root: None,
            by_person: false,
        }
    }

    fn a_report(outcome: Outcome) -> Handback {
        Handback {
            from: "check the queue".to_owned(),
            from_workspace: Place::Workspace("alpha".to_owned()),
            to: "steward 3".to_owned(),
            to_workspace: Place::Workspace("alpha".to_owned()),
            summary: "Forty are stuck.".to_owned(),
            task: Some(Reported {
                outcome,
                changed: None,
                record: None,
                stepped_in: false,
                by_person: false,
                unreported: false,
            }),
            answered: None,
            stopped: None,
        }
    }

    /// A chat on a measured harness that purlis has heard from.
    const KNOWN: Seen = Seen {
        ended: false,
        heard: true,
        running: false,
        waiting: false,
        asking: false,
        measured: true,
    };
    const RUNNING: Seen = Seen {
        running: true,
        ..KNOWN
    };
    const WAITING: Seen = Seen {
        waiting: true,
        ..KNOWN
    };
    /// Mid-turn, having shown the person a prompt: the board says waiting and asking.
    const ASKING: Seen = Seen {
        waiting: true,
        asking: true,
        ..KNOWN
    };
    const ENDED: Seen = Seen {
        ended: true,
        ..KNOWN
    };

    impl Seen {
        fn ended(self) -> Self {
            Seen {
                ended: true,
                ..self
            }
        }
    }

    // ----- whose task it is ----------------------------------------------------------------

    #[test]
    fn a_chat_owns_only_the_tasks_its_own_record_says_it_dispatched() {
        let mine = dispatched_by(ASKER);
        assert_eq!(owned(ASKER, TASK, Some(&mine)), Ok(&mine));
    }

    #[test]
    fn a_task_the_person_started_gives_the_chat_whose_tab_it_was_no_power_over_it() {
        // D-T59-j1. The record names the tab's chat, because the report goes there. That chat
        // dispatched nothing and holds no grant for the pair: every power is `owned`, so it has
        // none of them, in the sentence any stranger is told.
        let theirs = HandedFrom {
            by_person: true,
            ..dispatched_by(ASKER)
        };
        assert_eq!(owned(ASKER, TASK, Some(&theirs)), Err(not_yours(TASK)));
        assert_eq!(
            crate::dispatchtalk::down(ASKER, TASK, Some(&theirs)),
            Err(not_yours(TASK))
        );
        // And it cannot begin a cancel by another road: a cancel is recorded only for a record
        // `owned` answered, which this one is not.
        assert!(!Ledger::default().cancelling(TASK));
    }

    #[test]
    fn a_cancel_stands_down_for_good_when_the_person_stops_the_task() {
        // D-T59-j3: a cancelling chat can be stopped, and the stop is the later word.
        let mut ledger = cancelling();
        assert!(ledger.cancelling(TASK));

        ledger.stood_down(TASK);

        assert!(!ledger.cancelling(TASK));
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, true),
            Step::Nothing,
            "nothing more is sent for the cancel"
        );
        // Its last report is its own, not the cancel's.
        assert_eq!(
            ledger.outcome_for(TASK, Outcome::Blocked),
            Ok(Outcome::Blocked)
        );
        assert_eq!(
            being_stopped("check the queue", TASK),
            format!(
                "'check the queue' (chat {TASK}) is being stopped by the operator, so there is \
                 nothing to cancel. This chat is told when it has ended."
            )
        );
    }

    #[test]
    fn a_task_the_person_started_is_listed_and_said_to_be_theirs() {
        let row = |chat, by_person| Row {
            chat,
            name: "check prod".to_owned(),
            persona: Some("devops".to_owned()),
            place: "alpha".to_owned(),
            state: State::NeedsThePerson.say(),
            age_secs: Some(65),
            by_person,
        };
        let listed = list_text(&[row(8, false), row(9, true)]);
        assert_eq!(
            listed,
            format!(
                "2 tasks under this chat:\n\
                 - chat 8 · 'check prod' · devops · workspace 'alpha' · waiting on the operator · \
                 started 1m ago\n\
                 - chat 9 · 'check prod' · devops · workspace 'alpha' · waiting on the operator · \
                 started 1m ago{STARTED_BY_THE_PERSON}"
            )
        );
        assert!(list_text(&[row(8, false)]).starts_with("1 task dispatched by this chat:"));
    }

    #[test]
    fn a_sibling_a_parent_a_handoff_and_a_chat_that_is_not_there_are_refused_in_one_sentence() {
        // The forged asks of #1441: each names a chat the asker did not dispatch as a task.
        let siblings = dispatched_by(4);
        let handed_off = HandedFrom {
            mode: Mode::Handoff,
            ..dispatched_by(ASKER)
        };
        // The task asking about the chat that dispatched it: the parent's record names no asker,
        // or names the chat above it.
        let parents = dispatched_by(1);
        for (asker, of, record) in [
            (ASKER, TASK, Some(&siblings)),
            (ASKER, TASK, Some(&handed_off)),
            (TASK, ASKER, Some(&parents)),
            (TASK, ASKER, None),
            (ASKER, 77, None),
            // A record that names a chat as its own asker is nobody's task.
            (ASKER, ASKER, Some(&dispatched_by(ASKER))),
        ] {
            assert_eq!(
                owned(asker, of, record),
                Err(not_yours(of)),
                "{asker} asked about {of}"
            );
        }
        assert_eq!(
            not_yours(9),
            "chat 9 is not a task this chat dispatched, so this chat has nothing to do with it. \
             `purlis dispatch list` shows the tasks it did dispatch."
        );
    }

    #[test]
    fn an_ask_has_no_field_that_says_who_asks_but_the_chats_number() {
        let forged = r#"{"chat":4,"asker":3,"owner":3,"from":3,"what":{"wait":{"of":9,"within_secs":5,"asker":3}}}"#;
        let read: Asked = serde_json::from_str(forged).expect("it reads");
        assert_eq!(
            read,
            Asked {
                chat: 4,
                what: What::Wait {
                    of: 9,
                    within_secs: 5
                }
            }
        );
        let written = serde_json::to_value(&read).expect("json");
        let mut keys: Vec<&str> = written
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["chat", "what"]);
    }

    // ----- waiting -------------------------------------------------------------------------

    #[test]
    fn a_wait_goes_on_while_the_task_runs_and_ends_on_its_report() {
        let mut ledger = Ledger::default();
        let from = dispatched_by(ASKER);
        assert_eq!(ledger.waited(TASK, &from, RUNNING), None);
        assert_eq!(ledger.waited(TASK, &from, ASKING), None);
        assert_eq!(
            ledger.waited(TASK, &from, WAITING),
            None,
            "idle is not over"
        );

        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("f.json".into()));

        assert_eq!(
            ledger.waited(TASK, &from, RUNNING),
            Some(Waited::Reported {
                report: Box::new(a_report(Outcome::Done))
            })
        );
    }

    #[test]
    fn a_wait_on_a_task_that_ended_or_reported_before_a_relaunch_says_so_at_once() {
        let mut ledger = Ledger::default();
        assert_eq!(
            ledger.waited(TASK, &dispatched_by(ASKER), ENDED),
            Some(Waited::Ended)
        );
        let reported = HandedFrom {
            report: Owed::Sent,
            ..dispatched_by(ASKER)
        };
        assert_eq!(
            ledger.waited(TASK, &reported, RUNNING),
            Some(Waited::AlreadyRead)
        );
    }

    #[test]
    fn a_wait_is_held_for_what_it_asks_within_bounds() {
        assert_eq!(wait_secs(0), 1);
        assert_eq!(wait_secs(WAITS_BY_DEFAULT), 100);
        assert_eq!(wait_secs(u32::MAX), WAITS_AT_MOST);
        assert_eq!(WAITS_AT_MOST, 540);
    }

    #[test]
    fn a_report_a_wait_read_is_not_handed_to_the_next_turn_too() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("f.json".into()));

        assert_eq!(ledger.read(TASK), [PathBuf::from("f.json")]);
        assert!(ledger.read(TASK).is_empty(), "removed once");
        assert_eq!(ledger.nudge_step(ASKER, WAITING), Vec::new());
        // And it can be read again by a later wait.
        assert!(ledger.report(TASK).is_some());
    }

    // ----- being told ----------------------------------------------------------------------

    #[test]
    fn a_waiting_asking_chat_is_typed_one_line_for_the_reports_that_landed() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("a.json".into()));
        ledger.reported(12, ASKER, a_report(Outcome::Failed), Some("b.json".into()));

        assert_eq!(
            ledger.nudge_step(ASKER, WAITING),
            vec![Landed::Report(TASK), Landed::Report(12)]
        );
        assert_eq!(
            ledger.nudge_step(ASKER, WAITING),
            Vec::new(),
            "one line a batch, so a turn that hands over nothing is not typed into again"
        );
    }

    #[test]
    fn nothing_is_typed_into_a_chat_mid_turn_showing_a_prompt_ended_or_being_typed_in() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("a.json".into()));

        for seen in [
            RUNNING,
            ASKING,
            ENDED,
            // Never heard from: a chat still starting, or a harness that reports nothing.
            Seen {
                heard: false,
                ..WAITING
            },
            Seen::default(),
            // A harness purlis has not measured its line in.
            Seen {
                measured: false,
                ..WAITING
            },
        ] {
            assert_eq!(ledger.nudge_step(ASKER, seen), Vec::new(), "{seen:?}");
        }
        // M2: a key of the person's since its last report. They may have a picker open, which
        // no hook reports, so the line is held until the harness says a turn began or ended.
        ledger.person_keyed(ASKER);
        assert_eq!(ledger.nudge_step(ASKER, WAITING), Vec::new(), "keyed");
        assert!(ledger.keyed(ASKER));
        ledger.turn_ended(ASKER);
        // Held, not dropped: its turn ends, and it is told then.
        assert_eq!(
            ledger.nudge_step(ASKER, WAITING),
            vec![Landed::Report(TASK)]
        );
    }

    #[test]
    fn a_turn_the_person_began_was_handed_the_reports_so_no_line_follows() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("a.json".into()));

        ledger.turn_began(ASKER);

        assert_eq!(ledger.nudge_step(ASKER, WAITING), Vec::new());
    }

    #[test]
    fn a_report_kept_for_a_workspace_types_nothing_into_anyone() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), None);

        assert_eq!(ledger.nudge_step(ASKER, WAITING), Vec::new());
    }

    #[test]
    fn the_line_typed_is_purlis_s_own_and_names_chats_by_number_only() {
        assert_eq!(
            nudge(&[Landed::Report(9)]),
            "purlis: a task this chat dispatched has reported (chat 9). It is attached to this \
             turn as context, quoted as data. If it is not there, run `purlis dispatch wait 9`."
        );
        assert_eq!(
            nudge(&[Landed::Report(9), Landed::Report(12)]),
            "purlis: tasks this chat dispatched have reported (chats 9, 12). They are attached \
             to this turn as context, quoted as data. If one is not there, run `purlis dispatch \
             wait <chat>` for it."
        );
        // #1442: a question from a task, and a message from the chat above.
        assert_eq!(
            nudge(&[Landed::Question(9)]),
            "purlis: a task this chat dispatched asks it a question (chat 9). It is attached to \
             this turn as context, quoted as data. If it is not there, run `purlis dispatch \
             wait 9`."
        );
        assert_eq!(
            nudge(&[Landed::Answer]),
            "purlis: the chat that asked for this task has sent it a message. It is attached to \
             this turn as context, quoted as data."
        );
        assert_eq!(
            nudge(&[Landed::Report(9), Landed::FollowUp, Landed::Answer]),
            "purlis: a task this chat dispatched has reported (chat 9), and the chat that asked \
             for this task has sent it messages. They are attached to this turn as context, \
             quoted as data. If one is not there, run `purlis dispatch wait <chat>` for it."
        );
    }

    #[test]
    fn only_a_harness_measured_to_hand_the_turn_its_context_is_typed_the_line() {
        use crate::harness::Harness;
        assert!(told_by_a_line(Some(Harness::ClaudeCode)));
        assert!(told_by_a_line(Some(Harness::Codex)));
        assert!(!told_by_a_line(Some(Harness::Opencode)));
        assert!(!told_by_a_line(None), "a shell tab");
    }

    // ----- a question, a message and the person's keys (#1442) --------------------------------

    #[test]
    fn a_wait_is_answered_once_with_a_question_and_waits_on_after_it() {
        let mut ledger = Ledger::default();
        let from = dispatched_by(ASKER);
        ledger
            .talk
            .ask(TASK, "Which queue?", Some("q.json".into()))
            .expect("asked");
        assert_eq!(ledger.state(TASK, &from, RUNNING), State::AsksYou);
        assert_eq!(State::AsksYou.say(), "asking this chat a question");

        assert_eq!(
            ledger.waited(TASK, &from, RUNNING),
            Some(Waited::Asks {
                question: "Which queue?".to_owned()
            })
        );
        // The command has it: the turn after is not handed it, nor typed a line about it.
        ledger.landed(ASKER, Landed::Question(TASK));
        assert_eq!(ledger.read(TASK), [PathBuf::from("q.json")]);
        assert_eq!(ledger.nudge_step(ASKER, WAITING), Vec::new());
        assert_eq!(ledger.waited(TASK, &from, RUNNING), None, "waits on");
    }

    #[test]
    fn a_question_nobody_waited_for_is_what_the_asking_chat_is_typed_a_line_about() {
        let mut ledger = Ledger::default();
        ledger.landed(ASKER, Landed::Question(TASK));
        ledger.landed(ASKER, Landed::Question(TASK));

        assert_eq!(
            ledger.nudge_step(ASKER, WAITING),
            vec![Landed::Question(TASK)]
        );
    }

    #[test]
    fn an_answer_its_waiting_command_took_is_not_typed_about_and_a_follow_up_still_is() {
        let mut ledger = Ledger::default();
        ledger.talk.ask(TASK, "Which queue?", None).expect("asked");
        ledger
            .talk
            .answer(
                TASK,
                "check the queue",
                crate::dispatchtalk::Given {
                    from: "steward 3".to_owned(),
                    text: "The second.".to_owned(),
                    file: Some("a.json".into()),
                },
            )
            .expect("answered");
        ledger.landed(TASK, Landed::Answer);
        ledger.landed(TASK, Landed::FollowUp);

        assert_eq!(ledger.got_answer(TASK), Some(PathBuf::from("a.json")));

        assert_eq!(ledger.nudge_step(TASK, WAITING), vec![Landed::FollowUp]);
    }

    #[test]
    fn the_person_typing_in_a_task_is_remembered_as_a_fact_and_nothing_else() {
        let mut ledger = Ledger::default();
        assert!(!ledger.stepped_in(TASK));

        ledger.person_typed(TASK);

        assert!(ledger.stepped_in(TASK));
        assert!(!ledger.stepped_in(12), "another task's is its own");
        ledger.forget(TASK, None);
        assert!(!ledger.stepped_in(TASK));
    }

    // ----- the list ------------------------------------------------------------------------

    #[test]
    fn a_task_s_state_is_its_record_then_the_board() {
        let mut ledger = Ledger::default();
        let from = dispatched_by(ASKER);
        assert_eq!(ledger.state(TASK, &from, RUNNING), State::Running);
        assert_eq!(ledger.state(TASK, &from, WAITING), State::Idle);
        assert_eq!(ledger.state(TASK, &from, ASKING), State::NeedsThePerson);
        assert_eq!(ledger.state(TASK, &from, ENDED), State::Ended);

        let reported = HandedFrom {
            report: Owed::Sent,
            ..dispatched_by(ASKER)
        };
        assert_eq!(
            ledger.state(TASK, &reported, RUNNING),
            State::Reported(None)
        );
        ledger.reported(TASK, ASKER, a_report(Outcome::Blocked), None);
        assert_eq!(
            ledger.state(TASK, &reported, ENDED),
            State::Reported(Some(Outcome::Blocked)),
            "a report outlives its chat's program"
        );
        assert_eq!(
            State::Reported(Some(Outcome::Blocked)).say(),
            "reported: blocked"
        );
        assert_eq!(State::NeedsThePerson.say(), "waiting on the operator");
    }

    #[test]
    fn the_list_says_persona_task_where_state_and_age_for_each() {
        let rows = [
            Row {
                chat: 9,
                name: "check the queue".to_owned(),
                persona: Some("devops".to_owned()),
                place: "alpha".to_owned(),
                state: State::Running.say(),
                age_secs: Some(185),
                by_person: false,
            },
            Row {
                chat: 12,
                name: "read the logs".to_owned(),
                persona: None,
                place: Place::PlaneRoot.word().to_owned(),
                state: State::Reported(Some(Outcome::Done)).say(),
                age_secs: None,
                by_person: false,
            },
        ];

        assert_eq!(
            list_text(&rows),
            format!(
                "2 tasks dispatched by this chat:\n\
                 - chat 9 · 'check the queue' · devops · {} · running · started 3m ago\n\
                 - chat 12 · 'read the logs' · no persona · {} · reported: done · started \
                 before this app did",
                Place::Workspace("alpha".to_owned()).said(),
                Place::PlaneRoot.said()
            )
        );
        assert!(list_text(&[]).starts_with("This chat has no dispatched tasks open."));
    }

    #[test]
    fn an_age_is_said_in_its_two_largest_units() {
        for (secs, said) in [
            (0, "0s"),
            (45, "45s"),
            (60, "1m"),
            (3599, "59m"),
            (7500, "2h 5m"),
            (273_600, "3d 4h"),
        ] {
            assert_eq!(age_word(Duration::from_secs(secs)), said);
        }
        let mut ledger = Ledger::default();
        let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        ledger.started(TASK, then);
        assert_eq!(
            ledger.age(TASK, then + Duration::from_secs(90)),
            Some(Duration::from_secs(90))
        );
        assert_eq!(ledger.age(12, then), None, "started before this app did");
    }

    // ----- cancelling ----------------------------------------------------------------------

    fn cancelling() -> Ledger {
        let mut ledger = Ledger::default();
        assert_eq!(
            ledger.cancel(TASK, "check the queue", &dispatched_by(ASKER)),
            Ok(true)
        );
        ledger
    }

    #[test]
    fn a_running_task_is_interrupted_then_asked_for_a_short_report() {
        let mut ledger = cancelling();
        assert_eq!(
            ledger.state(TASK, &dispatched_by(ASKER), RUNNING),
            State::Cancelling
        );

        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false),
            Step::Interrupt
        );
        // No harness reports an interrupted turn: until it has had its moment, nothing more.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false),
            Step::Hold
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, true),
            Step::Prompt
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, true),
            Step::Hold,
            "typed once"
        );
    }

    #[test]
    fn a_task_whose_turn_had_ended_is_asked_at_once_and_never_interrupted() {
        let mut ledger = cancelling();

        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Prompt
        );
    }

    #[test]
    fn nothing_is_sent_to_a_task_showing_the_person_a_prompt_until_it_is_answered() {
        let mut ledger = cancelling();

        // Neither the interrupt nor the line: both are keys in its pane.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, ASKING, false),
            Step::Hold
        );
        ledger.person_keyed(TASK);
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Hold,
            "nor after a key of the person's in it"
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false),
            Step::Hold,
            "nor is its turn interrupted then"
        );
        // Its harness says a turn began: the key is behind it, and the turn is interrupted.
        ledger.turn_began(TASK);
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false),
            Step::Interrupt
        );
        // And a prompt that appears before the line is typed holds that too.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, ASKING, true),
            Step::Hold
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, true),
            Step::Prompt
        );
    }

    #[test]
    fn a_cancelled_task_reports_as_cancelled_whatever_its_chat_says() {
        let mut ledger = cancelling();

        for said in [Outcome::Done, Outcome::Failed, Outcome::Cancelled] {
            assert_eq!(ledger.outcome_for(TASK, said), Ok(Outcome::Cancelled));
        }
        // Its report ends the cancel, and the outcome stays the app's record.
        ledger.reported(TASK, ASKER, a_report(Outcome::Cancelled), None);
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Sent, WAITING, true),
            Step::Nothing
        );
        assert_eq!(
            ledger.outcome_for(TASK, Outcome::Done),
            Ok(Outcome::Cancelled)
        );
    }

    #[test]
    fn a_task_nobody_cancelled_cannot_say_it_was() {
        let ledger = Ledger::default();

        assert_eq!(
            ledger.outcome_for(TASK, Outcome::Cancelled),
            Err(NOT_CANCELLED.to_owned())
        );
        assert_eq!(
            ledger.outcome_for(TASK, Outcome::Blocked),
            Ok(Outcome::Blocked)
        );
    }

    #[test]
    fn a_cancelled_task_that_ends_its_one_turn_without_a_report_has_one_written_for_it() {
        let mut ledger = cancelling();
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Prompt
        );
        // Waiting still, because the line has not begun its turn yet: not the end of it.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Hold
        );

        ledger.turn_began(TASK);
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false),
            Step::Hold
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Write(CANCELLED_UNREPORTED)
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Nothing,
            "written once"
        );
    }

    #[test]
    fn a_cancelled_task_whose_program_has_ended_has_its_report_written_at_once() {
        let mut ledger = cancelling();

        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, ENDED, false),
            Step::Write(ENDED_UNREPORTED)
        );
        // Both sentences are ones a report may carry.
        for text in [ENDED_UNREPORTED, CANCELLED_UNREPORTED] {
            assert_eq!(crate::handoff::report_summary(text).as_deref(), Ok(text));
        }
    }

    #[test]
    fn a_task_that_has_reported_cannot_be_cancelled_and_a_second_cancel_changes_nothing() {
        let mut ledger = Ledger::default();
        let reported = HandedFrom {
            report: Owed::Sent,
            ..dispatched_by(ASKER)
        };
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), None);
        assert_eq!(
            ledger.cancel(TASK, "check the queue", &reported),
            Err(
                "'check the queue' (chat 9) has already reported (done), so there is nothing to \
                 cancel."
                    .to_owned()
            )
        );

        let mut ledger = cancelling();
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false),
            Step::Interrupt
        );
        assert_eq!(
            ledger.cancel(TASK, "check the queue", &dispatched_by(ASKER)),
            Ok(false)
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false),
            Step::Hold,
            "not interrupted twice"
        );
    }

    // ----- the review's probes (red first) ------------------------------------------------

    #[test]
    fn no_key_is_sent_to_a_chat_the_board_has_never_heard_from() {
        // M1: a task still starting, or on a harness that reports nothing, reads as neither
        // waiting nor asking. That is not a running turn.
        let mut ledger = cancelling();
        for settled in [false, true, true] {
            assert_eq!(
                ledger.cancel_step(TASK, Owed::Due, Seen::default(), settled),
                Step::Hold
            );
        }
        // Heard from, and idle with nothing known of a turn: still no Escape, which goes only
        // into a turn purlis heard begin.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, KNOWN, false),
            Step::Hold
        );
        // The cancel is recorded all the same: its report is `cancelled`, and one is written
        // when the chat ends.
        assert_eq!(
            ledger.outcome_for(TASK, Outcome::Done),
            Ok(Outcome::Cancelled)
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, Seen::default().ended(), false),
            Step::Write(ENDED_UNREPORTED)
        );
    }

    #[test]
    fn a_task_on_a_harness_purlis_does_not_type_into_is_cancelled_when_its_turn_ends() {
        let unmeasured = |seen: Seen| Seen {
            measured: false,
            ..seen
        };
        let mut ledger = cancelling();
        for seen in [unmeasured(RUNNING), unmeasured(ASKING)] {
            for settled in [false, true] {
                assert_eq!(
                    ledger.cancel_step(TASK, Owed::Due, seen, settled),
                    Step::Hold,
                    "{seen:?}"
                );
            }
        }

        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, unmeasured(WAITING), false),
            Step::Write(CANCELLED_UNASKED)
        );
        assert_eq!(
            crate::handoff::report_summary(CANCELLED_UNASKED).as_deref(),
            Ok(CANCELLED_UNASKED)
        );
    }

    #[test]
    fn a_cancel_line_no_turn_is_heard_to_begin_on_does_not_leave_the_task_cancelling_for_good() {
        // Fold 3.
        let mut ledger = cancelling();
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Prompt
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Hold
        );
        assert!(ledger.cancelling(TASK));

        ledger.prompt_unheard(TASK);

        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Write(CANCELLED_UNREPORTED)
        );
        assert!(!ledger.cancelling(TASK));
    }

    #[test]
    fn a_closed_task_is_remembered_by_how_it_ended_for_the_chat_that_asked() {
        // M6: a wait on a task whose tab was closed is the owner's, not a stranger's.
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), None);

        ledger.forget(TASK, Some((ASKER, "check the queue")));
        ledger.forget(12, Some((ASKER, "read the logs")));

        assert_eq!(
            ledger.gone(ASKER, TASK),
            Some(&Gone {
                asker: ASKER,
                name: "check the queue".to_owned(),
                report: Some(a_report(Outcome::Done)),
            })
        );
        assert_eq!(
            ledger.gone(ASKER, 12).and_then(|gone| gone.report.clone()),
            None
        );
        assert_eq!(ledger.gone(4, TASK), None, "and nobody else's");
        // Bounded, oldest out first; and gone with the chat that asked.
        for task in 100..100 + u32::try_from(MOST_GONE).unwrap() {
            ledger.forget(task, Some((ASKER, "more")));
        }
        assert_eq!(ledger.gone(ASKER, TASK), None);
        ledger.forget(ASKER, None);
        assert_eq!(ledger.gone(ASKER, 120), None);
    }

    #[test]
    fn a_chat_started_again_under_a_new_number_keeps_what_was_remembered_of_it() {
        // Fold 6: a restart to take a sandbox grant must not shed a cancel or a question.
        let mut ledger = cancelling();
        ledger.person_typed(TASK);
        ledger.talk.ask(TASK, "Which queue?", None).expect("asked");
        ledger.landed(ASKER, Landed::Question(TASK));
        ledger.landed(TASK, Landed::FollowUp);

        ledger.followed(TASK, 21);

        assert!(ledger.cancelling(21));
        assert!(ledger.stepped_in(21));
        assert_eq!(
            ledger.outcome_for(21, Outcome::Done),
            Ok(Outcome::Cancelled)
        );
        assert_eq!(ledger.talk.asks(21), Some("Which queue?"));
        assert!(!ledger.cancelling(TASK));
        assert_eq!(
            ledger.nudge_step(ASKER, WAITING),
            vec![Landed::Question(21)]
        );
        assert_eq!(ledger.nudge_step(21, WAITING), vec![Landed::FollowUp]);
        // And the asking chat started again still owns its tasks' news.
        ledger.landed(ASKER, Landed::Report(21));
        ledger.followed(ASKER, 30);
        assert_eq!(ledger.nudge_step(30, WAITING), vec![Landed::Report(21)]);
    }

    #[test]
    fn the_person_s_words_in_a_task_are_stepping_in_and_picking_an_option_is_not() {
        // M4. Outside a turn that asked them something, any key.
        assert!(steps_in(false, Some(0), b"x"));
        assert!(steps_in(false, None, b"\x1b[A"));
        // Within one: an option is a key or two and Enter.
        for keys in [&b"y"[..], b"2", b"\r", b"\x1b[B", b"\x1b", b"\t"] {
            assert!(!steps_in(true, Some(0), keys), "{keys:?}");
        }
        assert!(!steps_in(true, Some(1), b"\r"), "one key, then Enter");
        assert!(!steps_in(true, None, b"\r"), "arrows, then Enter");
        // An instruction is words: typed key by key, typed at once, or pasted.
        assert!(steps_in(true, Some(2), b"o"));
        assert!(steps_in(true, Some(0), b"no, use staging"));
        assert!(steps_in(true, Some(0), b"\x1b[200~use staging\x1b[201~"));
        assert!(steps_in(true, None, b"\x1b[200~use staging\x1b[201~"));
    }

    #[test]
    fn a_cancel_held_when_its_turn_had_stopped_moves_on_when_the_hold_lifts() {
        // Fold 2: the person was typing at the 1.5 s mark. The next look, with no timer behind
        // it, must still type the line.
        let mut ledger = cancelling();
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false),
            Step::Interrupt
        );
        ledger.person_keyed(TASK);
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, true),
            Step::Hold
        );
        ledger.turn_ended(TASK);
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false),
            Step::Prompt
        );
    }

    #[test]
    fn a_report_closes_the_question_its_task_had_open() {
        // Fold 4: an answer after the report would reach no turn of the work.
        let mut ledger = Ledger::default();
        ledger.talk.ask(TASK, "Which queue?", None).expect("asked");

        ledger.reported(TASK, ASKER, a_report(Outcome::Done), None);

        assert_eq!(ledger.talk.asks(TASK), None);
    }

    #[test]
    fn a_closed_chat_is_forgotten_as_a_task_and_as_an_asking_chat() {
        let mut ledger = Ledger::default();
        ledger.started(TASK, SystemTime::UNIX_EPOCH);
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("a.json".into()));

        ledger.forget(TASK, None);

        assert_eq!(ledger.report(TASK), None);
        assert_eq!(ledger.nudge_step(ASKER, WAITING), Vec::new());
    }
}
