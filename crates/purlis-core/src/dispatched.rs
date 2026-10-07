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
    match record {
        Some(from) if from.chat == asker && from.mode == Mode::Task && asker != of => Ok(from),
        _ => Err(not_yours(of)),
    }
}

// ---- where a task stands ----------------------------------------------------------------------

/// A chat as the app's board has it at this moment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Seen {
    /// Its program has ended, or the chat is not open.
    pub ended: bool,
    /// Its turn has ended and it is waiting to be prompted.
    pub waiting: bool,
    /// It is showing the person a prompt: a permission, or a question.
    pub asking: bool,
}

impl Seen {
    /// Whether a line may be typed into it: waiting for a prompt, and asking nothing. The gate
    /// a smart close's prompt is sent through, and every line purlis types into a chat.
    pub fn takes_a_line(self) -> bool {
        !self.ended && self.waiting && !self.asking
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
            Self::NeedsThePerson => "waiting on the person".to_owned(),
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
    /// By asking chat: the tasks whose reports landed and that it has not been told of.
    landed: HashMap<u32, Vec<u32>>,
}

#[derive(Debug, Default)]
struct Task {
    started: Option<SystemTime>,
    /// Its asking chat cancelled it. Kept once the cancel is over: it is the report's outcome.
    cancelled: bool,
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
    /// Not now: the chat is showing the person a prompt, the person is typing in it, or the
    /// line it was sent has not been answered yet. Asked again when the chat next moves.
    Hold,
    /// End the chat's turn ([`INTERRUPT`]), and ask again once it has had a moment to stop
    /// ([`A_TURN_STOPS_WITHIN`]).
    Interrupt,
    /// Type [`CANCEL_PROMPT`] into it.
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

/// What an outcome of `cancelled` from a chat nobody cancelled is told.
pub const NOT_CANCELLED: &str = "this task was not cancelled, and only a task its asking chat \
    cancelled reports that outcome. Use done, blocked or failed";

/// The line typed into an asking chat that is waiting, to say reports have landed. purlis's own
/// sentence and the app's numbers for the chats: nothing a chat wrote is in it.
pub fn nudge(tasks: &[u32]) -> String {
    match tasks {
        [one] => format!(
            "purlis: a task this chat dispatched has reported (chat {one}). Its report is \
             attached to this turn as context, quoted as data. If it is not there, run `purlis \
             dispatch wait {one}`."
        ),
        many => {
            let chats: Vec<String> = many.iter().map(u32::to_string).collect();
            format!(
                "purlis: tasks this chat dispatched have reported (chats {}). Their reports are \
                 attached to this turn as context, quoted as data. If one is not there, run \
                 `purlis dispatch wait <chat>` for it.",
                chats.join(", ")
            )
        }
    }
}

/// Whether an asking chat on `harness` is typed the line that says reports have landed: the
/// harnesses purlis arms a `UserPromptSubmit` hook on and has measured to hand that turn its
/// context. On any other, and in a chat with no harness, a report waits for the next turn the
/// person starts, as it did before this line existed.
pub fn told_by_a_line(harness: Option<crate::harness::Harness>) -> bool {
    use crate::harness::Harness;
    matches!(harness, Some(Harness::ClaudeCode | Harness::Codex))
}

impl Ledger {
    /// The app started the persona chat `task` at `now`.
    pub fn started(&mut self, task: u32, now: SystemTime) {
        self.tasks.entry(task).or_default().started = Some(now);
    }

    /// Chat `chat` has closed: nothing is remembered of it, as a task or as an asking chat.
    pub fn forget(&mut self, chat: u32) {
        self.tasks.remove(&chat);
        self.landed.remove(&chat);
        for told in self.landed.values_mut() {
            told.retain(|task| *task != chat);
        }
    }

    /// Whether anything here waits for chat `chat` to move: a cancel of it under way, or
    /// reports it has not been told of.
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
        let entry = self.tasks.entry(task).or_default();
        entry.cancel = None;
        if file.is_some() {
            let told = self.landed.entry(asker).or_default();
            if !told.contains(&task) {
                told.push(task);
            }
        }
        entry.report = Some(Kept { report, file });
    }

    /// The report `task` sent, where the app still has it.
    pub fn report(&self, task: u32) -> Option<&Kept> {
        self.tasks.get(&task)?.report.as_ref()
    }

    /// A waiting command has `task`'s report: the file it was left in for the asking chat's
    /// next turn, to remove, where it is still there. The asking chat is not told of it again.
    pub fn read(&mut self, task: u32) -> Option<PathBuf> {
        for told in self.landed.values_mut() {
            told.retain(|one| *one != task);
        }
        self.tasks.get_mut(&task)?.report.as_mut()?.file.take()
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
        } else if seen.asking {
            State::NeedsThePerson
        } else if seen.waiting {
            State::Idle
        } else {
            State::Running
        }
    }

    /// How a wait on `task` ends now, or none while there is still something to wait for.
    pub fn waited(&self, task: u32, from: &HandedFrom, seen: Seen) -> Option<Waited> {
        if let Some(kept) = self.report(task) {
            return Some(Waited::Reported {
                report: Box::new(kept.report.clone()),
            });
        }
        if from.report == Owed::Sent {
            return Some(Waited::AlreadyRead);
        }
        seen.ended.then_some(Waited::Ended)
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
    /// `owed` is what the task's record says it owes, `seen` what the board says of its chat,
    /// `typing` whether the person has a line of their own half typed in its pane, and
    /// `settled` whether [`A_TURN_STOPS_WITHIN`] has passed since its turn was interrupted.
    ///
    /// **Nothing is typed into a chat that is showing a prompt**, nor one the person is typing
    /// in: the step is [`Step::Hold`], and it is asked again when the chat next moves.
    pub fn cancel_step(
        &mut self,
        task: u32,
        owed: Owed,
        seen: Seen,
        typing: bool,
        settled: bool,
    ) -> Step {
        let Some(entry) = self.tasks.get_mut(&task) else {
            return Step::Nothing;
        };
        let Some(stage) = entry.cancel else {
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
        match stage {
            Cancel::Heard if seen.takes_a_line() => {
                entry.cancel = None;
                Step::Write(CANCELLED_UNREPORTED)
            }
            Cancel::Heard | Cancel::Prompted => Step::Hold,
            Cancel::Asked | Cancel::Interrupted => {
                if seen.asking || typing {
                    Step::Hold
                } else if seen.waiting || (stage == Cancel::Interrupted && settled) {
                    entry.cancel = Some(Cancel::Prompted);
                    Step::Prompt
                } else if stage == Cancel::Asked {
                    entry.cancel = Some(Cancel::Interrupted);
                    Step::Interrupt
                } else {
                    Step::Hold
                }
            }
        }
    }

    /// Chat `chat`'s harness said a prompt began a turn. As a cancelled task, the line it was
    /// typed has been taken; as an asking chat, the turn was handed every report that waited.
    pub fn turn_began(&mut self, chat: u32) {
        if let Some(entry) = self.tasks.get_mut(&chat)
            && entry.cancel == Some(Cancel::Prompted)
        {
            entry.cancel = Some(Cancel::Heard);
        }
        self.landed.remove(&chat);
    }

    /// The tasks the asking chat `asker` is typed a line about now, or none: reports have
    /// landed that it was not told of, it takes a line ([`Seen::takes_a_line`]), and the person
    /// is not `typing` in it. Taken, so one batch of reports is one line.
    pub fn nudge_step(&mut self, asker: u32, seen: Seen, typing: bool) -> Vec<u32> {
        if !seen.takes_a_line() || typing {
            return Vec::new();
        }
        self.landed.remove(&asker).unwrap_or_default()
    }
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

/// A chat's list of the tasks it dispatched, as `purlis dispatch list` prints it: persona,
/// task, where, state and age, one task a line.
pub fn list_text(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "This chat has no dispatched tasks open. A task is listed from its dispatch until \
                its chat is closed."
            .to_owned();
    }
    let mut said = format!(
        "{} dispatched by this chat:",
        if rows.len() == 1 {
            "1 task".to_owned()
        } else {
            format!("{} tasks", rows.len())
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
            "\n- chat {} · '{}' · {} · {place} · {} · {age}",
            row.chat,
            crate::personas::one_line(&row.name),
            match &row.persona {
                Some(persona) => crate::personas::one_line(persona),
                None => "no persona".to_owned(),
            },
            crate::personas::one_line(&row.state),
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
                by_person: false,
                stopped: false,
                unreported: false,
            }),
            answered: None,
        }
    }

    const RUNNING: Seen = Seen {
        ended: false,
        waiting: false,
        asking: false,
    };
    const WAITING: Seen = Seen {
        ended: false,
        waiting: true,
        asking: false,
    };
    const ASKING: Seen = Seen {
        ended: false,
        waiting: false,
        asking: true,
    };
    const ENDED: Seen = Seen {
        ended: true,
        waiting: false,
        asking: false,
    };

    // ----- whose task it is ----------------------------------------------------------------

    #[test]
    fn a_chat_owns_only_the_tasks_its_own_record_says_it_dispatched() {
        let mine = dispatched_by(ASKER);
        assert_eq!(owned(ASKER, TASK, Some(&mine)), Ok(&mine));
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
        let ledger = Ledger::default();
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

        assert_eq!(ledger.read(TASK), Some(PathBuf::from("f.json")));
        assert_eq!(ledger.read(TASK), None, "removed once");
        assert_eq!(ledger.nudge_step(ASKER, WAITING, false), Vec::<u32>::new());
        // And it can be read again by a later wait.
        assert!(ledger.report(TASK).is_some());
    }

    // ----- being told ----------------------------------------------------------------------

    #[test]
    fn a_waiting_asking_chat_is_typed_one_line_for_the_reports_that_landed() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("a.json".into()));
        ledger.reported(12, ASKER, a_report(Outcome::Failed), Some("b.json".into()));

        assert_eq!(ledger.nudge_step(ASKER, WAITING, false), vec![TASK, 12]);
        assert_eq!(
            ledger.nudge_step(ASKER, WAITING, false),
            Vec::<u32>::new(),
            "one line a batch, so a turn that hands over nothing is not typed into again"
        );
    }

    #[test]
    fn nothing_is_typed_into_a_chat_mid_turn_showing_a_prompt_ended_or_being_typed_in() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("a.json".into()));

        for (seen, typing) in [
            (RUNNING, false),
            (ASKING, false),
            (
                Seen {
                    waiting: true,
                    asking: true,
                    ended: false,
                },
                false,
            ),
            (ENDED, false),
            (WAITING, true),
        ] {
            assert_eq!(
                ledger.nudge_step(ASKER, seen, typing),
                Vec::<u32>::new(),
                "{seen:?}, typing {typing}"
            );
        }
        // Held, not dropped: its turn ends, and it is told then.
        assert_eq!(ledger.nudge_step(ASKER, WAITING, false), vec![TASK]);
    }

    #[test]
    fn a_turn_the_person_began_was_handed_the_reports_so_no_line_follows() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("a.json".into()));

        ledger.turn_began(ASKER);

        assert_eq!(ledger.nudge_step(ASKER, WAITING, false), Vec::<u32>::new());
    }

    #[test]
    fn a_report_kept_for_a_workspace_types_nothing_into_anyone() {
        let mut ledger = Ledger::default();
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), None);

        assert_eq!(ledger.nudge_step(ASKER, WAITING, false), Vec::<u32>::new());
    }

    #[test]
    fn the_line_typed_is_purlis_s_own_and_names_chats_by_number_only() {
        assert_eq!(
            nudge(&[9]),
            "purlis: a task this chat dispatched has reported (chat 9). Its report is attached \
             to this turn as context, quoted as data. If it is not there, run `purlis dispatch \
             wait 9`."
        );
        assert_eq!(
            nudge(&[9, 12]),
            "purlis: tasks this chat dispatched have reported (chats 9, 12). Their reports are \
             attached to this turn as context, quoted as data. If one is not there, run `purlis \
             dispatch wait <chat>` for it."
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
        assert_eq!(State::NeedsThePerson.say(), "waiting on the person");
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
            },
            Row {
                chat: 12,
                name: "read the logs".to_owned(),
                persona: None,
                place: Place::PlaneRoot.word().to_owned(),
                state: State::Reported(Some(Outcome::Done)).say(),
                age_secs: None,
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
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, false),
            Step::Interrupt
        );
        // No harness reports an interrupted turn: until it has had its moment, nothing more.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, false),
            Step::Hold
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, true),
            Step::Prompt
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, true),
            Step::Hold,
            "typed once"
        );
    }

    #[test]
    fn a_task_whose_turn_had_ended_is_asked_at_once_and_never_interrupted() {
        let mut ledger = cancelling();

        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false, false),
            Step::Prompt
        );
    }

    #[test]
    fn nothing_is_sent_to_a_task_showing_the_person_a_prompt_until_it_is_answered() {
        let mut ledger = cancelling();

        // Neither the interrupt nor the line: both are keys in its pane.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, ASKING, false, false),
            Step::Hold
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, true, false),
            Step::Hold,
            "nor while the person is typing in it"
        );
        // The person answered and the turn went on: now it is interrupted.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, false),
            Step::Interrupt
        );
        // And a prompt that appears before the line is typed holds that too.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, ASKING, false, true),
            Step::Hold
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, true),
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
            ledger.cancel_step(TASK, Owed::Sent, WAITING, false, true),
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
            ledger.cancel_step(TASK, Owed::Due, WAITING, false, false),
            Step::Prompt
        );
        // Waiting still, because the line has not begun its turn yet: not the end of it.
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false, false),
            Step::Hold
        );

        ledger.turn_began(TASK);
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, false),
            Step::Hold
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false, false),
            Step::Write(CANCELLED_UNREPORTED)
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, WAITING, false, false),
            Step::Nothing,
            "written once"
        );
    }

    #[test]
    fn a_cancelled_task_whose_program_has_ended_has_its_report_written_at_once() {
        let mut ledger = cancelling();

        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, ENDED, false, false),
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
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, false),
            Step::Interrupt
        );
        assert_eq!(
            ledger.cancel(TASK, "check the queue", &dispatched_by(ASKER)),
            Ok(false)
        );
        assert_eq!(
            ledger.cancel_step(TASK, Owed::Due, RUNNING, false, false),
            Step::Hold,
            "not interrupted twice"
        );
    }

    #[test]
    fn a_closed_chat_is_forgotten_as_a_task_and_as_an_asking_chat() {
        let mut ledger = Ledger::default();
        ledger.started(TASK, SystemTime::UNIX_EPOCH);
        ledger.reported(TASK, ASKER, a_report(Outcome::Done), Some("a.json".into()));

        ledger.forget(TASK);

        assert_eq!(ledger.report(TASK), None);
        assert_eq!(ledger.nudge_step(ASKER, WAITING, false), Vec::<u32>::new());
    }
}
