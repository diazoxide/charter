//! What a chat's state is, and the only thing allowed to move it.
//!
//! Nothing here reads a harness's output (ADR 0018, spec decision 3). A state changes for
//! exactly two reasons: a harness hook reported an [`Event`], or the session's own program
//! exited — which is the process telling the app, not a screen charter read.
//!
//! [`run`] is where a RUN's state lives (ADR 0076): nine states, each move naming one cause.

pub mod children;
pub mod run;

use crate::harness::model::{Ask, Began, Item, Said, Session, Turn};
use crate::hookwire::Conversation;

/// A chat's state, as the sidebar draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No state hook has ever reported. The harness carries none, or has not started.
    Unknown,
    /// A turn is in flight.
    Running,
    /// The harness has handed control back: it asked something, or its turn ended.
    Waiting,
    /// The session itself ended, and its program was content.
    Done,
    /// The session's program exited non-zero.
    Failed,
}

/// What a `SessionStart` was for, where the harness said.
///
/// Claude Code's payload carries `source`. Only two of its four values are a session
/// beginning; `compact` fires in the middle of a turn, and `clear` ends one. Guessing from
/// the state charter happened to be holding got one of the two wrong whichever way round it
/// was written, which is how this field came to be carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Started {
    /// A session is here: `startup`, or `resume`.
    Freshly,
    /// The conversation was cleared. The turn, if there was one, is over.
    Cleared,
    /// The conversation was compacted. Nothing about the turn changed.
    Compacted,
    /// No `source` was readable. Treated as a session beginning, which is what a
    /// `SessionStart` means when nothing says otherwise.
    #[default]
    Unsaid,
}

impl Started {
    /// Whether this is a session arriving, as opposed to something happening inside one.
    ///
    /// Read by the app's curation, which types a prompt only into a session that has just
    /// begun. It is answered by the neutral model ([`Event::said`]), so the board and the
    /// curation can never disagree about which `SessionStart` began a session.
    pub fn began_a_session(self) -> bool {
        let detail = Detail {
            started: self,
            ..Detail::default()
        };
        matches!(
            Event::SessionStart.said(detail),
            Said::Session(Session::Began(_))
        )
    }

    /// What Claude Code's `source` means, measured on 2.1.276 (`startup` seen live; the
    /// others are the harness's own documented values). Codex sends the same field with the
    /// same words — `startup`, and `resume` on `codex resume <id>`, both seen on codex-cli
    /// 0.147.0 (#27).
    pub fn of(source: Option<&str>) -> Self {
        match source {
            Some("startup" | "resume") => Self::Freshly,
            Some("clear") => Self::Cleared,
            Some("compact") => Self::Compacted,
            _ => Self::Unsaid,
        }
    }
}

/// What a `SessionEnd` was for, where the harness said.
///
/// **`/clear` fires `SessionEnd` before `SessionStart`, and this is measured, not assumed.**
/// On claude 2.1.276, typing `/clear` produces `SessionEnd(reason=clear)` on the old
/// conversation and then `SessionStart(source=clear)` on the new one, from the same process.
/// A `SessionEnd` that ends the chat would therefore end every cleared chat — the C6 "follow"
/// would be unreachable, and the chat would read `done` until its process exited. A review
/// predicted it from the `reason` field's existence; the measurement settled it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ending {
    /// The conversation was cleared. The SESSION did not end, and another begins at once.
    Cleared,
    /// The session itself is over.
    #[default]
    ForGood,
}

impl Ending {
    /// What Claude Code's `reason` means, measured on 2.1.276: `clear` seen live across a
    /// `/clear`, and `other` on the way out of a `-p` run.
    pub fn of(reason: Option<&str>) -> Self {
        match reason {
            Some("clear") => Self::Cleared,
            _ => Self::ForGood,
        }
    }
}

/// Everything a harness said about an event beyond its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Detail {
    pub started: Started,
    pub ending: Ending,
    /// On a `UserPromptSubmit`: the prompt was `/smart-close` ([`smart_close_typed`]). Never the
    /// prompt itself: the app hears one bit of it, and only this bit. Absent from an older hook,
    /// which reads as no. It never issues a pass on its own: the app issues one on the line the
    /// person typed into the chat's pane, and this bit can only withhold it (#1332, #1361).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub smart_close: bool,
    /// The harness said it runs with its permission prompts off
    /// ([`crate::floorguard::unattended`] of the payload's `permission_mode`): one bit, which
    /// the app keeps as the chat's [`crate::dispatchunattended::Mark`] (#1446). It can only
    /// ever mark a chat unattended, never clear the mark, so a line that leaves it out, as an
    /// older hook's does, takes nothing away.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unattended: bool,
}

/// Whether a prompt a harness hands its `UserPromptSubmit` hook is the person typing purlis's
/// smart-close skill as a slash command: `/smart-close`, or with the plugin's prefix
/// (`/purlis:smart-close`, or the old name's), alone or with words after it. Anything else, a
/// sentence that only mentions it included, is not (#1332).
pub fn smart_close_typed(prompt: &str) -> bool {
    let Some(command) = prompt.split_whitespace().next() else {
        return false;
    };
    let Some(command) = command.strip_prefix('/') else {
        return false;
    };
    let skill = std::iter::once(crate::names::SKILL_NAMESPACE.write)
        .chain(crate::names::SKILL_NAMESPACE.reads.iter().copied())
        .find_map(|prefix| command.strip_prefix(prefix))
        .unwrap_or(command);
    skill == SMART_CLOSE_SKILL
}

/// The smart-close skill's name, as a person types it after the `/`.
pub const SMART_CLOSE_SKILL: &str = "smart-close";

/// A state-carrying hook, by the event a harness fires it on.
///
/// On the wire it is the word `charter hook` takes, so a report is readable by eye.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    /// A session began — or was cleared, or compacted. [`Report::started`] says which.
    SessionStart,
    UserPromptSubmit,
    Notification,
    SubagentStop,
    Stop,
    SessionEnd,
}

impl Event {
    /// The event this word names, or none. The word is what `charter hook <word>` takes, and
    /// it is the harness's own event name lowercased — the shape the Python charter's
    /// `hooks/hooks.json` already uses (`charter hook sessionstart`).
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "sessionstart" => Self::SessionStart,
            "userpromptsubmit" => Self::UserPromptSubmit,
            "notification" => Self::Notification,
            "subagentstop" => Self::SubagentStop,
            "stop" => Self::Stop,
            "sessionend" => Self::SessionEnd,
            // Every other word is a TOOL hook (`hookreg`), answered by the guards and never
            // reported: a tool call carries no chat state the app draws.
            _ => return None,
        })
    }

    /// What this hook word means in the neutral harness model (ADR 0073), with `detail`, what
    /// its payload said beyond the event's name.
    ///
    /// The words are charter's own, the same for every harness: each adapter's hooks run
    /// `charter hook <word>`, so this reading is not any one harness's. It sits beside the
    /// words, so the model knows nothing of them.
    pub fn said(self, detail: Detail) -> Said {
        match self {
            Self::SessionStart => Said::Session(match detail.started {
                Started::Freshly | Started::Unsaid => Session::Began(Began::Fresh),
                Started::Cleared => Session::Began(Began::Cleared),
                Started::Compacted => Session::Compacted,
            }),
            Self::UserPromptSubmit => Said::Turn(Turn::Began),
            Self::Notification => Said::Ask(Ask::default()),
            Self::SubagentStop => Said::Item(Item::ChildEnded),
            Self::Stop => Said::Turn(Turn::Ended),
            Self::SessionEnd => Said::Session(match detail.ending {
                Ending::Cleared => Session::ClearedAway,
                Ending::ForGood => Session::Ended,
            }),
        }
    }

    /// The word `charter hook` takes for this event.
    pub fn word(self) -> &'static str {
        match self {
            Self::SessionStart => "sessionstart",
            Self::UserPromptSubmit => "userpromptsubmit",
            Self::Notification => "notification",
            Self::SubagentStop => "subagentstop",
            Self::Stop => "stop",
            Self::SessionEnd => "sessionend",
        }
    }
}

/// Why a chat needs the person when no hook of its own says so (#1448): something the app
/// found out about it. Each is a needs-you item on that chat, which says the reason, until the
/// chat's next prompt, the person's Ignore, or the chat's end.
///
/// **Where a new reason goes.** A chat waiting on a dispatch grant is the next one (#1437): a
/// variant here, raised with [`Board::needs`], and a sentence for it in the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Need {
    /// The chat reported, and its report has nowhere to go: the chat that asked for it,
    /// `asker` as the person saw it, has gone. The report is kept for that chat's workspace.
    ReportUndelivered { asker: String },
}

/// **What a chat whose turn has ended is waiting on that is not the person** (#1491), as the
/// app's own records say at the moment its harness speaks: the tasks below it, and the chat
/// that dispatched it. The board holds no lineage of its own, so whoever applies a report says
/// ([`Board::reported_while`]); a report applied with nothing said waits on nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Waits {
    /// How many tasks below it, at any depth, are still working, asking, or owing a report.
    pub tasks: u32,
    /// Whether it is a task paused on a question to the chat that dispatched it: that chat is
    /// the one who can answer, and its row reads `asking <chat>`. Only while that chat is open
    /// and its program runs: a question to a chat that has gone is nobody's to answer.
    pub its_asker: bool,
    /// Whether a line of purlis's own is about to start a turn of it: a report or a question
    /// landed that it has not been told of and it takes a line, or the line was typed and its
    /// harness has not yet said the turn began. The end of this turn is then not the person's:
    /// the next turn starts by itself, and its end is.
    pub line_coming: bool,
}

impl Waits {
    /// Whether the end of its turn leaves the next move with somebody other than the person.
    pub fn on_something(self) -> bool {
        self.tasks > 0 || self.its_asker || self.line_coming
    }
}

/// **A task that came to nothing, as the chat that asked for it is flagged for it** (#1491,
/// V100-15): it failed, it ended without a report, or it did not start. A needs-you item on
/// the asking chat that says which task and why, whatever that chat and its other tasks are
/// doing. A task that finished as done, or was cancelled, is none: it changes a count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedTask {
    /// What names this failure, and nothing else does: its dispatch record's id, which its
    /// finished row carries too, or a word of the app's own for a task that never had a
    /// record. Two tasks of one name are two failures.
    pub id: String,
    /// The task's chat, by number, where it is still open (a task that reported blocked
    /// stays open as the chat it is): what going to the failure shows.
    pub chat: Option<u32>,
    /// The task, by the name the person sees it under.
    pub task: String,
    /// Which of the three it was.
    pub how: HowFailed,
    /// Why, in a few words ([`FailedTask::in_a_few_words`]): what its report said, or what
    /// refused its start. Empty where nothing says.
    pub why: String,
}

/// How a task came to nothing ([`FailedTask`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HowFailed {
    /// It reported that it failed, or that it was blocked: it did not do the work.
    Failed,
    /// Its program ended while it still owed its report.
    Unreported,
    /// It was asked for and never started.
    DidNotStart,
}

impl HowFailed {
    /// The word the window is sent for it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Unreported => "unreported",
            Self::DidNotStart => "did_not_start",
        }
    }
}

impl FailedTask {
    /// The most characters of a reason an item says: a few words, never a report.
    pub const WHY_AT_MOST: usize = 60;

    /// The failure `id` names: `task` came to nothing as `how`, for `why` cut to a few words.
    pub fn new(id: &str, task: &str, how: HowFailed, why: &str) -> Self {
        Self {
            id: id.to_owned(),
            chat: None,
            task: task.to_owned(),
            how,
            why: Self::in_a_few_words(why),
        }
    }

    /// `text` as a few words: its first line that says anything, on one line, cut at
    /// [`Self::WHY_AT_MOST`] characters with an ellipsis where it was longer.
    pub fn in_a_few_words(text: &str) -> String {
        let line = text
            .lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .find(|line| !line.is_empty())
            .unwrap_or_default();
        if line.chars().count() <= Self::WHY_AT_MOST {
            return line;
        }
        let cut: String = line.chars().take(Self::WHY_AT_MOST - 1).collect();
        format!("{}…", cut.trim_end())
    }

    /// This failure, of a task whose chat `chat` is still open.
    #[must_use]
    pub fn of_open_chat(mut self, chat: u32) -> Self {
        self.chat = Some(chat);
        self
    }
}

/// Where a chat stands with the person at one moment ([`Board::standing`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Standing {
    /// Whether it is in the needs-you queue.
    pub queued: bool,
    /// How many reasons of the app's own it is there for.
    pub reasons: usize,
}

/// **Whether a move from `was` to `now` raised something for the person** (#1491): the chat
/// came into the queue, or a reason was added to one already in it (a second task failed, a
/// commit was refused). A chat that only moved while its item stood raised nothing, and
/// neither did one that left the queue.
pub fn raised(was: Standing, now: Standing) -> bool {
    now.queued && (!was.queued || now.reasons > was.reasons)
}

/// One chat's state, and everything that is allowed to move it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    state: State,
    /// Whether the harness has asked for the operator, as opposed to merely not running.
    needs_you: bool,
    /// Whether the session's program is gone. Hooks run as the harness's children and can
    /// outlive it by a moment; one that lands afterwards must not resurrect the chat.
    ///
    /// **Only [`Chat::exited`] sets this, and that is the fix for a defect a review found.**
    /// `SessionEnd` used to set it too, which made a cleared chat unreachable for the rest of
    /// its life — and let anything in the chat's own process tree freeze a WORKING chat at
    /// `done` with one forged report. The exit status comes from the operating system and
    /// cannot be forged, and it already does this job: it overwrites whatever the board
    /// holds, so a dead chat can never look alive.
    ended: bool,
    /// The chats that have reported back to this one since it was last prompted, by the name
    /// the operator sees them under (charter-app#259). The report itself reaches this chat's
    /// next turn as context, which is why the next prompt is what clears them. **Not a
    /// needs-you item** (#1448): a report goes to the chat that asked, not to the person. A
    /// chat that is in the queue for a reason of its own says `<child> reported back` there.
    reports: Vec<String>,
    /// The chats this one started that the operator stopped since it was last prompted, by
    /// name (#1448). purlis's own word, kept apart from `reports`, which are what chats said:
    /// the row says `<child> was stopped`. No needs-you item, and cleared as reports are.
    stopped: Vec<String>,
    /// Why the chat needs the person when no hook of its own said so ([`Need`], #1448).
    needs: Vec<Need>,
    /// Whether the chat has reported to the chat that asked, in the turn it is in (#1448). Its
    /// turn ending then waits on that chat and not on the person, so it raises no needs-you
    /// item. The next prompt starts new work and clears it.
    reported: bool,
    /// The commits of this chat charter's `pre-commit` refused since it was last prompted, each
    /// as the one masked line the item says (SQ-16, [`crate::diffscan`]). A needs-you item of
    /// its own kind, like a report back, and the next prompt clears them the same way.
    refusals: Vec<String>,
    /// Whether the chat stopped in the middle of a turn to ask the operator something — a
    /// permission or a question — and the turn has not ended since (Smart close, ADR 0064).
    ///
    /// **A `Notification` while a turn runs, and only then.** Claude Code also nudges a chat
    /// left idle after its turn with a `Notification`, and that one asks nothing: the turn is
    /// over and the chat is only waiting. Both put the chat in the needs-you queue, which is
    /// why this is a fact of its own and not `needs_you`. Not part of what a reader is shown
    /// (the `was` of [`Chat::reported_from`]), so it never makes a report a move of its own.
    asking: bool,
    /// How many prompts have started a turn of this chat since the app started it.
    turns: u32,
    /// The child agents of its current run, shown under it (FD-18, W8).
    children: children::Children,
    /// Whether its turn ended while it waited on something that is not the person ([`Waits`],
    /// #1491): a task below it still at work, or the answer of the chat that dispatched it. No
    /// needs-you item was raised for that end. Its next prompt clears it; [`Chat::rested`]
    /// turns it into the item when what it waited on is over and nothing will prompt it.
    held: bool,
    /// The tasks it asked for that came to nothing and the person has not looked at yet
    /// ([`FailedTask`], #1491), oldest first. **Each is a needs-you item of its own kind, and
    /// its next prompt does not clear it**: the chat is typed a line when a report lands, so a
    /// reason its next turn cleared would be gone before anybody saw it. The person's look
    /// ([`Chat::failures_seen`]), their Ignore, a cleared row and the chat's end clear it.
    failed: Vec<FailedTask>,
}

impl Chat {
    /// A chat nothing has reported yet.
    pub fn new() -> Self {
        Self {
            state: State::Unknown,
            needs_you: false,
            ended: false,
            reports: Vec::new(),
            stopped: Vec::new(),
            needs: Vec::new(),
            reported: false,
            refusals: Vec::new(),
            asking: false,
            turns: 0,
            children: children::Children::default(),
            held: false,
            failed: Vec::new(),
        }
    }

    /// The child agents of the chat's current run, oldest first.
    pub fn children(&self) -> &[children::Child] {
        self.children.list()
    }

    /// A hook of child agent `agent` was heard that says nothing more than that it is there:
    /// a tool call. Answers whether anything a reader can see changed.
    pub fn child_heard(&mut self, agent: &str) -> bool {
        !self.ended && self.children.seen(agent)
    }

    /// Child agent `agent` said `said`, on its parent's hooks. Answers whether anything a
    /// reader can see changed.
    ///
    /// **Only its ask is the chat's** (ADR 0076 §6): a child is never `input-required`, so
    /// what it asks the operator, the chat asks. Its own turn, its session and its end move
    /// the child alone, so a fan-out of sub-agents never blinks the chat out of `running`.
    pub fn child_said(&mut self, agent: &str, said: &Said) -> bool {
        if self.ended {
            return false;
        }
        match said {
            Said::Item(Item::ChildEnded) => self.children.ended(agent),
            Said::Ask(_) => {
                let seen = self.children.seen(agent);
                self.heard(said) || seen
            }
            _ => self.children.seen(agent),
        }
    }

    /// Whether the chat is asking the operator something in the middle of a turn: a
    /// permission or a question, never the nudge of a chat that has finished its turn.
    pub fn asking(&self) -> bool {
        self.asking
    }

    /// How many prompts have started a turn of this chat, as the app has heard them.
    pub fn turns(&self) -> u32 {
        self.turns
    }

    /// The state as it stands.
    pub fn state(&self) -> State {
        self.state
    }

    /// Whether this chat belongs in the "needs you" queue.
    ///
    /// **The queue's rules, in one place** (#1448, #1491):
    ///
    /// - **A turn that ends hands the chat to the person**, and so does the nudge of a chat
    ///   left idle. That is the falling edge the queue is for.
    /// - **Unless the next move is somebody else's.** A chat that reported to the chat that
    ///   asked waits on that chat. A chat whose turn ended while a task below it, at any
    ///   depth, is still working, asking or owing its report waits on its tasks, and its row
    ///   says `waiting on n tasks`. A task paused on a question to the chat that dispatched it
    ///   waits on that chat's answer, and its row says `asking <chat>`. None of the three is
    ///   an item ([`Waits`], [`Chat::heard_while`]).
    /// - **A chat that waited on its tasks becomes an item only once every one of them has
    ///   reported or ended and it has then stopped with nothing to do**: it is typed a line
    ///   when a report lands, reads it in a turn of its own, and that turn's end is the item.
    ///   A turn that ends with a line of purlis's own about to start the next is held too.
    /// - **A chat that needs the person is never left at rest with no hand.** The hold is
    ///   only a hold: wherever what a chat waits on may have gone, the app asks its records
    ///   again and, where nothing is left and nothing will prompt the chat, says so
    ///   ([`Chat::rested`]). Held in memory, and gone with the app, as every item is.
    /// - **A real prompt always is an item, whatever its tasks are doing**: a permission or a
    ///   question asked mid-turn ([`Chat::asking`]), a reason the app found ([`Need`]), a
    ///   refused commit, and a task of its that failed, ended without a report or did not
    ///   start ([`FailedTask`]). A task that finished as done, or was cancelled, is none.
    /// - **The person's Ignore drops the item until the chat asks again** ([`Chat::ignored`]),
    ///   and a chat whose program has ended asks for nobody.
    pub fn needs_you(&self) -> bool {
        self.needs_you || !self.failed.is_empty()
    }

    /// Whether its turn ended waiting on its tasks or on its asker, and no item was raised for
    /// it ([`Waits`]).
    pub fn held(&self) -> bool {
        self.held
    }

    /// The tasks it asked for that came to nothing and the person has not looked at, oldest
    /// first.
    pub fn failed_tasks(&self) -> &[FailedTask] {
        &self.failed
    }

    /// A task this chat asked for failed, ended without a report, or did not start (#1491): a
    /// needs-you item on this chat that says which and why, whatever it is doing, and its
    /// state is not touched. Answers whether anything a reader can see changed: nothing does
    /// for a failure it already says.
    pub fn task_failed(&mut self, failed: FailedTask) -> bool {
        if self.ended || self.failed.iter().any(|one| one.id == failed.id) {
            return false;
        }
        self.failed.push(failed);
        true
    }

    /// The person looked at what failed below this chat: the items for it go, and whatever
    /// else the chat needs them for stays. Answers whether anything a reader can see changed.
    pub fn failures_seen(&mut self) -> bool {
        !std::mem::take(&mut self.failed).is_empty()
    }

    /// The person looked at failure `id`, or cleared its row: that one item goes, and every
    /// other stays. Answers whether anything a reader can see changed.
    pub fn failure_cleared(&mut self, id: &str) -> bool {
        let was = self.failed.len();
        self.failed.retain(|failed| failed.id != id);
        was != self.failed.len()
    }

    /// How many reasons of the app's own it is an item for: what it needs the person for
    /// beyond its own hooks, its refused commits and its failed tasks. One more than before
    /// is a reason raised ([`Board::standing`]).
    fn reasons(&self) -> usize {
        self.needs.len() + self.refusals.len() + self.failed.len()
    }

    /// **What this chat waited on is over, and nothing will prompt it** (#1491): every task
    /// below it has reported or ended, or its asker answered, and the app typed it no line.
    /// The end of turn that was held is the needs-you item now. Nothing for a chat no end was
    /// held for, one that has begun a turn since, or one that reported to its asker. Answers
    /// whether anything a reader can see changed.
    pub fn rested(&mut self) -> bool {
        if self.ended || !self.held || self.state != State::Waiting || self.reported {
            return false;
        }
        let was = self.seen();
        self.held = false;
        self.needs_you = true;
        was != self.seen()
    }

    /// The chats that reported back to this one and have not been read yet, oldest first.
    pub fn reports(&self) -> &[String] {
        &self.reports
    }

    /// What a reader of this chat is shown, so a move can be told from no move by comparing
    /// it before and after. [`Chat::asking`] and the turn count are not part of it.
    fn seen(&self) -> (State, bool, usize, usize, usize, usize, usize) {
        (
            self.state,
            self.needs_you(),
            self.reports.len(),
            self.refusals.len(),
            self.needs.len(),
            self.stopped.len(),
            self.failed.len(),
        )
    }

    /// The chats this one started that the operator stopped and it has not been told of yet,
    /// oldest first.
    pub fn stopped(&self) -> &[String] {
        &self.stopped
    }

    /// The operator stopped `from`, a chat this one started (#1448). The word waits for this
    /// chat's next turn, as a report does, and is no needs-you item. Answers whether anything
    /// a reader can see changed.
    pub fn stopped_below(&mut self, from: &str) -> bool {
        if self.ended {
            return false;
        }
        self.stopped.push(from.to_owned());
        true
    }

    /// Why the chat needs the person beyond what its own hooks said, oldest first.
    pub fn needs(&self) -> &[Need] {
        &self.needs
    }

    /// The app found that this chat needs the person, for `need` (#1448). A needs-you item
    /// whatever the chat is doing, and its state is not touched. Answers whether anything a
    /// reader can see changed: nothing does for a need it already has.
    pub fn needs_the_person(&mut self, need: Need) -> bool {
        if self.ended {
            return false;
        }
        let was = self.seen();
        if !self.needs.contains(&need) {
            self.needs.push(need);
        }
        self.needs_you = true;
        was != self.seen()
    }

    /// This chat's report reached the chat that asked for it (#1448). The turn it is in then
    /// ends waiting on that chat: no needs-you item is raised for it, and the nudge of a chat
    /// left idle raises none either. A question it asks mid-turn still does.
    pub fn reported_to_its_asker(&mut self) {
        if !self.ended {
            self.reported = true;
        }
    }

    /// The commits of this chat that were refused and not yet seen, oldest first.
    pub fn refusals(&self) -> &[String] {
        &self.refusals
    }

    /// charter's `pre-commit` refused a commit this chat made (SQ-16): `what` is the masked
    /// line the item says. It needs the operator whatever it is doing, and its state is not
    /// touched — the agent has git's refusal in front of it and its turn goes on. Answers
    /// whether anything a reader can see changed.
    pub fn commit_refused(&mut self, what: &str) -> bool {
        if self.ended {
            return false;
        }
        self.refusals.push(what.to_owned());
        self.needs_you = true;
        true
    }

    /// A chat this one handed work to, `from`, has reported back (charter-app#259). The report
    /// waits for this chat's next turn. Answers whether anything a reader can see changed.
    ///
    /// **Not a needs-you item** (#1448): the report is this chat's to read, not the person's.
    /// **And not its state.** A chat in the middle of a turn is still running; the report does
    /// not end the turn and nothing is typed into it.
    pub fn reported_back(&mut self, from: &str) -> bool {
        if self.ended {
            return false;
        }
        self.reports.push(from.to_owned());
        true
    }

    /// A hook reported `event`. Answers whether anything a reader can see changed.
    pub fn reported(&mut self, event: Event) -> bool {
        self.reported_from(event, Detail::default())
    }

    /// The same, with what the harness said about the event beyond its name.
    pub fn reported_from(&mut self, event: Event, detail: Detail) -> bool {
        self.heard(&event.said(detail))
    }

    /// The harness said `said`, in the neutral model (ADR 0073), at whatever level it runs.
    /// Answers whether anything a reader can see changed.
    pub fn heard(&mut self, said: &Said) -> bool {
        self.heard_while(said, Waits::default())
    }

    /// [`Chat::heard`], while the chat waits on `waits`: what the end of its turn leaves the
    /// next move with ([`Chat::needs_you`] has the rule).
    pub fn heard_while(&mut self, said: &Said, waits: Waits) -> bool {
        if self.ended {
            return false;
        }
        let was = self.seen();
        match said {
            // A fresh chat, and every chat a relaunch puts back, wants a first prompt. It
            // has asked for nothing, and a launch that filled the queue would empty the
            // queue of its meaning.
            //
            // Nothing at all: a compaction is not a new session. Claude Code fires
            // `SessionStart` with a `source` of `startup`, `resume`, `clear` or `compact`,
            // and auto-compact fires it in the MIDDLE of a turn — which used to put the chat
            // back to `waiting` while the agent was still working. A review found that, and
            // then found that guessing from "is a turn running" got `/clear` wrong in the
            // other direction. The payload says which, so nothing has to be guessed.
            Said::Session(Session::Compacted) => {}
            Said::Session(Session::Began(_)) => {
                self.state = State::Waiting;
                self.asking = false;
            }
            // The turn that begins now is the one a waiting report is handed to, as context
            // (`handback`), so it is read and no longer waiting.
            Said::Turn(Turn::Began) => {
                self.state = State::Running;
                self.needs_you = false;
                self.reports.clear();
                self.stopped.clear();
                self.refusals.clear();
                self.needs.clear();
                self.reported = false;
                self.asking = false;
                self.held = false;
                self.turns = self.turns.saturating_add(1);
            }
            // The turn has not ended, but it cannot go on without an answer.
            Said::Ask(_) => {
                // Asked in the middle of a turn. After one has ended it is only a nudge.
                self.asking = self.asking || self.state == State::Running;
                self.state = State::Waiting;
                // The nudge of a chat that reported to the chat that asked is not it waiting
                // on the person (#1448), and neither is the nudge of one that waits on its
                // tasks or on its asker's answer (#1491). A question mid-turn always is.
                if self.asking || !(self.reported || waits.on_something()) {
                    self.needs_you = true;
                } else if !self.reported {
                    self.held = true;
                }
            }
            // The falling edge: the agent has nothing more to do, so the next move is the
            // operator's. After a `Notification` the STATE does not change and the reason
            // does, which is why the queue is answered separately from the state.
            //
            // Unconditional, and a surviving mutant is why. This used to queue only a chat
            // whose `UserPromptSubmit` the app had seen — a guard for a sequence no harness
            // fires (nothing produces a `Stop` before a prompt), so no test could reach it.
            // The case it would really have caught is the app MISSING a prompt event, and
            // there the guard gives the wrong answer: a turn has still ended, and the
            // operator still has the next move.
            //
            // **But not for a chat that reported to the chat that asked, in this turn**
            // (#1448): the next move is that chat's. What it already needed the person for
            // stays.
            //
            // **And not for a chat that waits on its tasks, or on its asker's answer**
            // (#1491): the end is held, and is the item only once what it waits on is over
            // and it has stopped with nothing to do.
            Said::Turn(Turn::Ended) => {
                self.state = State::Waiting;
                match (self.reported, waits.on_something()) {
                    (true, _) => {}
                    (false, true) => self.held = true,
                    (false, false) => self.needs_you = true,
                }
                self.asking = false;
            }
            // Emphatically not `Stop`: a dispatched sub-agent finishing does not end the
            // turn that dispatched it, and a fan-out would blink the chat out of `running`
            // several times over (`charter/hooks.py:stop`).
            Said::Item(Item::ChildEnded) => {}
            // **`/clear` ends a CONVERSATION, not the session** — measured on claude 2.1.276,
            // where typing it fires `SessionEnd(reason=clear)` and then
            // `SessionStart(source=clear)` from the same process. The `SessionStart` that
            // follows says what the chat is now.
            Said::Session(Session::ClearedAway) => {}
            Said::Session(Session::Ended) => {
                self.state = State::Done;
                self.needs_you = false;
                self.asking = false;
            }
            // Neither is a state a reader is shown, at any level.
            Said::Plan(_) | Said::Usage(_) => {}
        }
        was != self.seen()
    }

    /// The operator dismissed this chat's request without answering it (charter-app#248).
    /// Answers whether anything a reader can see changed.
    ///
    /// **Only the request goes; the chat is still `Waiting`**, because it is: nothing was
    /// typed into it. And that is what makes "until the chat asks again" free: the next `Stop`
    /// or `Notification` sets `needs_you` again, and the pair it is compared on differs, so a
    /// new request on a chat that never stopped waiting still counts as a change.
    ///
    /// Held nowhere but here, so a relaunch forgets it — and a relaunch starts with nothing
    /// asking anyway (charter-app#247).
    ///
    /// A report back (charter-app#259) is a request of its own kind and goes with it: the item
    /// leaves the queue. The report itself still reaches the chat's next turn — ignoring the
    /// item is not unreading what another chat said.
    pub fn ignored(&mut self) -> bool {
        let had_reports = !self.reports.is_empty()
            || !self.refusals.is_empty()
            || !self.needs.is_empty()
            || !self.stopped.is_empty()
            || !self.failed.is_empty();
        self.failed.clear();
        self.stopped.clear();
        self.reports.clear();
        self.refusals.clear();
        self.needs.clear();
        std::mem::replace(&mut self.needs_you, false) || had_reports
    }

    /// The person answered, in the window, the prompt this chat showed in the middle of a turn
    /// (HP-6). The turn goes on: it is running again and no longer waits on them, and `asking`
    /// stays set until the turn ends, as it does for a prompt answered anywhere. Nothing for a
    /// chat that was not asking mid-turn. Answers whether anything a reader can see changed.
    pub fn answered(&mut self) -> bool {
        if self.ended || !self.asking || self.state != State::Waiting {
            return false;
        }
        let was = self.seen();
        self.state = State::Running;
        self.needs_you = false;
        was != self.seen()
    }

    /// The session's program exited. Answers whether anything a reader can see changed.
    ///
    /// No hook reports this and none can — the process is gone. An exit status is the
    /// program telling the app directly, which is not the harness OUTPUT that ADR 0018
    /// forbids reading.
    pub fn exited(&mut self, code: Option<i32>) -> bool {
        let was = self.seen();
        // No code at all is what a signal leaves behind, and that is not a clean end.
        self.state = if code == Some(0) {
            State::Done
        } else {
            State::Failed
        };
        self.needs_you = false;
        self.asking = false;
        self.held = false;
        self.ended = true;
        // Nothing will prompt it again, so nothing it was waiting to read is an item any more.
        self.reports.clear();
        self.stopped.clear();
        self.refusals.clear();
        self.needs.clear();
        self.failed.clear();
        // A child still working ends with its parent, in the parent's end (ADR 0076 §6). It
        // shares the parent's process group, so the stop that ended the parent ended it.
        let children = self.children.end_with(self.state);
        was != self.seen() || children
    }
}

impl Default for Chat {
    fn default() -> Self {
        Self::new()
    }
}

/// What every chat is doing, in one place: the app's whole answer to "which one needs me".
///
/// It is the only thing that applies a [`Report`](crate::hookwire::Report), and it applies one
/// only to a chat the app started, from the conversation the app started it under.
#[derive(Debug, Default)]
pub struct Board {
    chats: std::collections::BTreeMap<u32, Tracked>,
}

/// How many times anything on any board in this process has moved. See [`Board::moved_at`].
///
/// **One count for every board, not one each**, because each project has its own board and
/// the project strip's show-more menu orders projects by their newest move (ADR 0054,
/// charter#401). Counts taken per board cannot be compared across them: a project that moved
/// fifty times an hour ago would read above one that moved once just now.
static MOVES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[derive(Debug)]
struct Tracked {
    /// The board's count at the last move a reader could see. See [`Board::moved_at`].
    moved_at: u32,
    chat: Chat,
    /// The conversation this chat holds, where charter knows it.
    ///
    /// Claude Code is started under an id charter chose, so this is set before the harness
    /// exists. Codex names no flag to choose one (openai/codex#14482), so it arrives with the
    /// first report and is adopted then (ADR 0024, decision 2).
    conversation: Option<String>,
    /// The harness pid this chat has adopted, once one has reported.
    ///
    /// Learned from a report and never from the process charter started: behind a wrapper
    /// profile the process charter started is the wrapper's, not the harness's (ADR 0024).
    pid: Option<u32>,
    /// Whether a report has been adopted for this chat yet.
    adopted: bool,
    /// Whether this chat's harness reports its own process id.
    ///
    /// The chat's, not the report's: it is what decides which rule a report is judged by, and
    /// letting a report choose its own rulebook is how a nested `claude` took a Codex chat.
    reports_pid: bool,
}

impl Tracked {
    /// Whether this report is this chat's harness speaking, adopting or following it if so.
    ///
    /// **Which rulebook applies is decided by the CHAT's harness, never by the report's own
    /// shape.** A review found the version that branched on whether the report carried a pid:
    /// a `claude` started inside a Codex chat's shell carries one, so it took the
    /// pid-bearing branch, found nothing to contradict it, and adopted the chat — after
    /// which the real Codex was refused for the rest of the chat's life. The Python charter
    /// cannot reach that, because `_record_harness_session` drops a report whose harness is
    /// not the chat's before `_record_harness_report` ever sees it, and its docstring names
    /// this exact case.
    fn take(&mut self, report: &crate::hookwire::Report) -> bool {
        let said = match &report.conversation {
            Conversation::Named(said) => Some(said.clone()),
            // Something is reporting a conversation that is not the one its own process is
            // in. That is never this chat's harness, adopted or not.
            Conversation::Contradicted => return false,
            // A harness speaking a dialect charter cannot read. Before this chat has adopted
            // anything there is nothing to compare it against and the event is taken; after,
            // the chat has a harness and this is not it — a nested `opencode` inherits
            // `CLAUDE_PID` and names its conversation under a key charter does not read, so
            // without this it marked the outer chat waiting, mid-turn.
            Conversation::Foreign if self.adopted => return false,
            Conversation::Foreign => None,
            // Charter read nothing at all — its own deadline, or a cut pipe. Not evidence of
            // anything; the pid below is what says whose report this is.
            Conversation::Unknown => None,
        };
        // A malformed id must not be able to use up the one report an adoption gets
        // (`charter/hooks.py:_record_harness_report`, which validates it first and says why).
        if said
            .as_deref()
            .is_some_and(|said| crate::harness::SessionId::new(said).is_err())
        {
            return false;
        }
        if self.reports_pid {
            self.claude_code(said, report.pid)
        } else {
            self.naming_its_own(said, report.pid)
        }
    }

    /// A harness that reports its process — Claude Code. Adopt, follow, or ignore, by pid.
    fn claude_code(&mut self, said: Option<String>, pid: Option<u32>) -> bool {
        // "No default: a report with no `$CLAUDE_PID` … before adoption it never adopts,
        // after it it is not the adopted pid" (`charter/hooks.py`).
        let Some(pid) = pid else { return false };
        match self.pid {
            // Nothing adopted yet. It must be the conversation charter chose, where charter
            // chose one — a report of some other id, before adoption, is not this chat's.
            None => {
                let Some(said) = said else { return false };
                if self
                    .conversation
                    .as_ref()
                    .is_some_and(|known| *known != said)
                {
                    return false;
                }
                self.pid = Some(pid);
                self.conversation = Some(said);
                self.adopted = true;
                true
            }
            // A different pid is a harness running INSIDE the chat (C5). It reports its own
            // conversation and must move nothing.
            Some(adopted) if pid != adopted => false,
            // The adopted process. It IS this chat's harness, so the event counts whatever
            // charter managed to read of the payload — an id it has not seen before is
            // `/clear` (C6) and the chat follows it; none at all leaves the link alone.
            //
            // Taking the event here is the fix for a regression an earlier version had: a
            // slow payload made `conversation` unreadable and the chat's OWN `Stop` was then
            // dropped, leaving it `running` for ever.
            Some(_) => {
                if let Some(said) = said {
                    self.conversation = Some(said);
                }
                true
            }
        }
    }

    /// A harness that names no process — Codex, and anything else charter has measured.
    ///
    /// The first report of a chat is adopted and a later different id is ignored: nothing in
    /// a Codex report tells a nested run from a new conversation, so a new one is not
    /// followed (ADR 0024).
    fn naming_its_own(&mut self, said: Option<String>, pid: Option<u32>) -> bool {
        // A report that names a process is not from this harness at all — it is a Claude
        // Code running inside this chat's shell. Python refuses it one layer earlier, by the
        // chat's recorded `CHARTER_HARNESS`.
        if pid.is_some() {
            return false;
        }
        let Some(said) = said else {
            // Before anything is adopted there is nothing to check an unreadable id against,
            // so the event counts; afterwards the chat has an id and this is not it.
            return !self.adopted;
        };
        if self.adopted {
            return self.conversation.as_ref() == Some(&said);
        }
        if self
            .conversation
            .as_ref()
            .is_some_and(|known| *known != said)
        {
            return false;
        }
        self.conversation = Some(said);
        self.adopted = true;
        true
    }
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    /// The app started chat `number` running `harness`, under `conversation` where it chose
    /// one.
    ///
    /// `harness` is what decides how a report is judged. A program charter has NOT measured
    /// gets the narrowest rule there is — the pid-less one below, where the first report of a
    /// chat is adopted and a later different conversation is ignored.
    ///
    /// **Not a blanket refusal, and this was tried.** A review pointed out that the doc here
    /// once claimed such a chat "reports nothing", and suggested making the code match. It
    /// does not, because `Harness::of_command` cannot tell a plain shell from a harness
    /// charter has not measured yet — they are the same answer — so refusing one refuses the
    /// other, and a harness that genuinely reports its state would be shown as `unknown` for
    /// ever. The scenario tests proved it in the most direct way available: their own fake
    /// harness went dark.
    ///
    /// What is left is that something in a shell chat's own process tree can move that chat.
    /// ADR 0024 concedes exactly that class for every chat — "a `claude -p` in the chat's own
    /// shell carries them too" — so this is not a new hole, and the cost of closing it here
    /// is a harness that cannot speak at all.
    pub fn opened(
        &mut self,
        number: u32,
        harness: Option<crate::harness::Harness>,
        conversation: Option<String>,
    ) {
        let moved_at = self.moved();
        self.chats.insert(
            number,
            Tracked {
                moved_at,
                chat: Chat::new(),
                conversation,
                pid: None,
                adopted: false,
                reports_pid: harness.is_some_and(crate::harness::Harness::reports_its_process),
            },
        );
    }

    /// The chat is gone from the app altogether — closed, not merely ended.
    pub fn closed(&mut self, number: u32) {
        self.chats.remove(&number);
    }

    /// Applies one report. Answers whether anything a reader can see changed.
    ///
    /// **Adopt, follow, or ignore** — the Python charter's rule, ported field for field from
    /// `charter/hooks.py:_record_harness_report` and ADR 0024:
    ///
    /// * **adopt** — nothing adopted yet, and the report is of the id charter chose for this
    ///   chat (C1, C3) or of any id where charter chose none. Its pid becomes the chat's.
    /// * **follow** — a different id from the ADOPTED PID is `/clear` (C6): the operator is
    ///   in a new conversation and the chat moves with them.
    /// * **ignore** — anything else: a different pid is a `claude` running inside the chat
    ///   (C5), and it must not be able to move the chat it is running in.
    ///
    /// The pid is the whole of what tells C6 from C5 — both report an id the chat has not
    /// seen. An earlier version of this file carried no pid, could not tell them apart, and
    /// answered both as "ignore": a chat that was `/clear`ed then showed one state for the
    /// rest of its life. An independent review found it.
    ///
    /// **Two deliberate divergences from Python**, both because this answers a different
    /// question — what a chat is DOING, where Python is maintaining the tab-to-session link:
    ///
    /// * Python refuses any report whose payload has no well-formed id. This takes one from
    ///   the adopted process, because charter's own read deadline is the commonest reason a
    ///   payload cannot be read and dropping the event leaves the chat `running` for ever. A
    ///   payload that PARSED and named no conversation charter reads is a different thing
    ///   ([`Conversation::Foreign`]) and is still refused.
    /// * Python records only the FIRST report of a Codex start (`adopt_report` is `O_EXCL`).
    ///   This accepts every later report naming the same conversation, because a state board
    ///   needs every `Stop`, not only the first.
    ///
    /// One case both share and neither closes: `charter hook stop` typed in the chat's own
    /// shell moves the chat. ADR 0024 concedes that class outright — "a `claude -p` in the
    /// chat's own shell carries them too" — and nothing in a report can distinguish it.
    pub fn reported(&mut self, report: &crate::hookwire::Report) -> bool {
        self.reported_while(report, Waits::default())
    }

    /// [`Board::reported`], while the chat the report names waits on `waits` (#1491): what the
    /// app's own records say is below it and above it at this moment. The board holds no
    /// lineage, so the one who applies the report reads it and says. A sub-agent's report is
    /// its chat's ask and never the end of its turn, so `waits` is not asked of it.
    pub fn reported_while(&mut self, report: &crate::hookwire::Report, waits: Waits) -> bool {
        let Some(tracked) = self.chats.get_mut(&report.chat) else {
            // A report for a chat the app does not have: one that was closed a moment ago,
            // or a stale environment in a process that outlived it.
            return false;
        };
        let was = tracked.conversation.clone();
        if !tracked.take(report) {
            return false;
        }
        // A sub-agent naming another conversation is not the chat's `/clear`: the chat stays in
        // the one it was in, and keeps its children.
        if report.agent.is_some() && was.is_some() {
            tracked.conversation.clone_from(&was);
        }
        // The chat followed its own harness onto another conversation (C6): a new run began
        // in the same process, and the old run's children were the old run's.
        let mut changed =
            was.is_some() && tracked.conversation != was && tracked.chat.children.superseded();
        let said = report.event.said(report.detail);
        changed |= match &report.agent {
            Some(agent) => tracked.chat.child_said(agent, &said),
            None => tracked.chat.heard_while(&said, waits),
        };
        self.stamp(report.chat, changed)
    }

    /// What chat `number` waited on is over, and nothing will prompt it ([`Chat::rested`]).
    /// Answers whether anything a reader can see changed: nothing does for a chat the board
    /// does not have, or one no end of turn was held for.
    pub fn rested(&mut self, number: u32) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.rested());
        self.stamp(number, changed)
    }

    /// Whether chat `number`'s turn ended waiting on its tasks or its asker ([`Chat::held`]).
    pub fn held(&self, number: u32) -> bool {
        self.chats
            .get(&number)
            .is_some_and(|tracked| tracked.chat.held())
    }

    /// A task chat `number` asked for came to nothing ([`Chat::task_failed`]). Answers whether
    /// anything a reader can see changed: nothing does for a chat the board does not have, or
    /// one whose program is gone.
    pub fn task_failed(&mut self, number: u32, failed: FailedTask) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.task_failed(failed));
        self.stamp(number, changed)
    }

    /// The tasks chat `number` asked for that came to nothing and the person has not looked
    /// at, oldest first.
    pub fn failed_tasks(&self, number: u32) -> Vec<FailedTask> {
        self.chats
            .get(&number)
            .map(|tracked| tracked.chat.failed_tasks().to_vec())
            .unwrap_or_default()
    }

    /// The person looked at what failed below chat `number` ([`Chat::failures_seen`]).
    ///
    /// **Not stamped as a move**, as an Ignore is not ([`Board::ignored`]): looking at a chat
    /// is not something the chat did.
    pub fn failures_seen(&mut self, number: u32) -> bool {
        self.chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.failures_seen())
    }

    /// The person looked at failure `id` of chat `number`, or cleared its row
    /// ([`Chat::failure_cleared`]). Not stamped as a move, as an Ignore is not.
    pub fn failure_cleared(&mut self, number: u32, id: &str) -> bool {
        self.chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.failure_cleared(id))
    }

    /// **Where chat `number` stands with the person**: whether it is in the queue, and how
    /// many reasons of the app's own it is there for. Read before and after a move, it says
    /// whether the move is one to interrupt the person for ([`raised`]): a system notification
    /// is sent on the rising edge, never for a chat that merely moved while an item stood.
    pub fn standing(&self, number: u32) -> Standing {
        self.chats
            .get(&number)
            .map_or(Standing::default(), |tracked| Standing {
                queued: tracked.chat.needs_you(),
                reasons: tracked.chat.reasons(),
            })
    }

    /// A tool hook of child agent `agent` of chat `number` was heard: a Codex child's first
    /// hook is one, since charter arms Codex with no `SubagentStop`. Answers whether anything a
    /// reader can see changed: nothing does for a chat the board does not have.
    ///
    /// **Not checked against the chat's conversation** as a report is, because a tool call
    /// carries none. It is admitted by the chat's own token, as the event log admits it, so
    /// a sub-agent of a harness nested in the chat's shell can show as a child of the chat:
    /// a row that is wrong, never a state of the chat that is.
    pub fn child_heard(&mut self, number: u32, agent: &str) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.child_heard(agent));
        self.stamp(number, changed)
    }

    /// The child agents of chat `number`'s current run, oldest first; none for a chat the
    /// board does not have.
    pub fn children(&self, number: u32) -> Vec<children::Child> {
        self.chats
            .get(&number)
            .map(|tracked| tracked.chat.children().to_vec())
            .unwrap_or_default()
    }

    /// The operator ignored this chat's request (charter-app#248). Answers whether anything a
    /// reader can see changed: `false` for a chat that was not asking, or is not on the board.
    ///
    /// **Not stamped as a move.** [`Board::moved_at`] orders chats by what THEY last did
    /// (ADR 0039), and the operator dismissing a request is not something the chat did.
    pub fn ignored(&mut self, number: u32) -> bool {
        self.chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.ignored())
    }

    /// The person answered chat `number`'s prompt in the window ([`Chat::answered`]). Answers
    /// whether anything a reader can see changed.
    pub fn answered(&mut self, number: u32) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.answered());
        self.stamp(number, changed)
    }

    /// The chat's program exited. Answers whether anything a reader can see changed.
    pub fn exited(&mut self, number: u32, code: Option<i32>) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.exited(code));
        self.stamp(number, changed)
    }

    /// When this chat last moved, as a count of moves on every board in this process — bigger
    /// is more recent, on this board and against any other ([`MOVES`]).
    ///
    /// **A count and not a clock, deliberately.** The only thing anything asks of it is an
    /// order: ADR 0039 puts the chat strip's overflow menu in last-activity order,
    /// and an order is all that needs. Three things follow from choosing the weaker fact:
    ///
    /// * There is no clock to disagree with. Two chats that moved in the same millisecond
    ///   still have an order, and nothing depends on the machine's time going forwards.
    /// * It is a `u32`, which is what can cross the app's boundary. `specta` refuses to
    ///   export a `u64` and the app panics at startup in a debug build when one is reached
    ///   for, so the obvious spelling of a wall-clock stamp is one this codebase cannot
    ///   carry.
    /// * A number that cannot be rendered as a date will not be drawn as one. "3 minutes
    ///   ago" is a claim this board cannot make — a hook fires when a turn moves, not on a
    ///   timer — and a field that could be formatted that way is one that eventually would
    ///   be.
    ///
    /// **It advances only when a reader would see a difference**, which is the same
    /// condition [`Board::reported`] and [`Board::exited`] already answer. A hook that
    /// fires and changes nothing pushes no event to any window, so a stamp that moved on it
    /// would be one no window could ever learn — and making every hook an event is the cost
    /// ADR 0039 names as the one not to pay ("a field on an event that already fires is
    /// cheap; a new event is not").
    ///
    /// **The cost of that, stated:** a long turn that fires fifty hooks and stays `running`
    /// throughout moved once, when it started running. The order is "which chat's state
    /// changed most recently", not "which chat is busiest", and those differ for exactly
    /// that chat.
    ///
    /// A chat the board does not have reads `0`. Every chat it does have has moved at least
    /// once, because opening it is a move — so `0` means "not on this board", and a reader
    /// sorting on it puts such a chat last.
    pub fn moved_at(&self, number: u32) -> u32 {
        self.chats
            .get(&number)
            .map_or(0, |tracked| tracked.moved_at)
    }

    /// Stamps `number` with a fresh move when `changed`, and answers `changed` unaltered.
    fn stamp(&mut self, number: u32, changed: bool) -> bool {
        if changed {
            let moved_at = self.moved();
            if let Some(tracked) = self.chats.get_mut(&number) {
                tracked.moved_at = moved_at;
            }
        }
        changed
    }

    /// The next move's count.
    ///
    /// Saturating rather than wrapping: at four billion moves the order stops improving,
    /// which is a menu in the wrong order, where a wrap would put the newest chat first at
    /// the top of the list one move and last the next. Nothing reaches it — a hook event a
    /// second for a century is three billion — and the arithmetic is written down anyway
    /// because `u32` is a boundary this file chose rather than one it was given.
    fn moved(&mut self) -> u32 {
        use std::sync::atomic::Ordering;
        // `try_update` only fails when the closure answers `None`, and this one never does,
        // so both arms hold the count as it was before this move.
        let was = MOVES
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |was| {
                Some(was.saturating_add(1))
            })
            .unwrap_or_else(|was| was);
        was.saturating_add(1)
    }

    /// The conversation this chat holds NOW, where charter knows one — the id charter started
    /// it under, or the one its own harness moved to on `/clear` (C6), which [`Tracked::take`]
    /// follows. `None` for a chat the board does not have, or one whose harness has not named
    /// a conversation yet.
    ///
    /// **Now, and not the one it was started under**, because the thing it names moves: Claude
    /// Code's `statusLine` payload carries the CURRENT session id, so the usage it records
    /// after a `/clear` is filed under the new one. A reader keyed on the start id would
    /// draw the conversation the operator cleared away. And never a nested harness's: the
    /// board refuses to follow a report from any process but the chat's own (C5).
    pub fn conversation(&self, number: u32) -> Option<&str> {
        self.chats.get(&number)?.conversation.as_deref()
    }

    /// What this chat is doing. A chat the app does not have is [`State::Unknown`], which is
    /// what it looks like from outside.
    pub fn state(&self, number: u32) -> State {
        self.chats
            .get(&number)
            .map_or(State::Unknown, |tracked| tracked.chat.state())
    }

    /// Whether this chat is asking the operator something mid-turn ([`Chat::asking`]). A chat
    /// the board does not have asks nothing.
    pub fn asking(&self, number: u32) -> bool {
        self.chats
            .get(&number)
            .is_some_and(|tracked| tracked.chat.asking())
    }

    /// How many prompts have started a turn of this chat ([`Chat::turns`]); none for a chat the
    /// board does not have.
    pub fn turns(&self, number: u32) -> u32 {
        self.chats
            .get(&number)
            .map_or(0, |tracked| tracked.chat.turns())
    }

    /// A chat `number` handed work to, `from`, has reported back to it (charter-app#259).
    /// Answers whether anything a reader can see changed: nothing does for a chat the board
    /// does not have, or one whose program is gone.
    pub fn reported_back(&mut self, number: u32, from: &str) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.reported_back(from));
        self.stamp(number, changed)
    }

    /// The operator stopped `from`, a chat `number` started (#1448). Answers whether anything
    /// a reader can see changed: nothing does for a chat the board does not have, or one whose
    /// program is gone.
    pub fn stopped_below(&mut self, number: u32, from: &str) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.stopped_below(from));
        self.stamp(number, changed)
    }

    /// The chats `number` started that the operator stopped and it has not been told of yet.
    pub fn stopped_of(&self, number: u32) -> Vec<String> {
        self.chats
            .get(&number)
            .map(|tracked| tracked.chat.stopped().to_vec())
            .unwrap_or_default()
    }

    /// The app found that chat `number` needs the person, for `need` (#1448). Answers whether
    /// anything a reader can see changed: nothing does for a chat the board does not have, or
    /// one whose program is gone.
    pub fn needs(&mut self, number: u32, need: Need) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.needs_the_person(need));
        self.stamp(number, changed)
    }

    /// Why chat `number` needs the person beyond what its own hooks said, oldest first.
    pub fn needs_of(&self, number: u32) -> Vec<Need> {
        self.chats
            .get(&number)
            .map(|tracked| tracked.chat.needs().to_vec())
            .unwrap_or_default()
    }

    /// Chat `number`'s report reached the chat that asked for it
    /// ([`Chat::reported_to_its_asker`]). Nothing a reader can see changes.
    pub fn reported_to_its_asker(&mut self, number: u32) {
        if let Some(tracked) = self.chats.get_mut(&number) {
            tracked.chat.reported_to_its_asker();
        }
    }

    /// charter's `pre-commit` refused a commit chat `number` made (SQ-16). Answers whether
    /// anything a reader can see changed: nothing does for a chat the board does not have, or
    /// one whose program is gone.
    pub fn commit_refused(&mut self, number: u32, what: &str) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.commit_refused(what));
        self.stamp(number, changed)
    }

    /// The refused commits of chat `number` not yet seen, oldest first.
    pub fn refusals(&self, number: u32) -> Vec<String> {
        self.chats
            .get(&number)
            .map(|tracked| tracked.chat.refusals().to_vec())
            .unwrap_or_default()
    }

    /// The chats that reported back to `number` and have not been read, oldest first.
    pub fn reports(&self, number: u32) -> Vec<String> {
        self.chats
            .get(&number)
            .map(|tracked| tracked.chat.reports().to_vec())
            .unwrap_or_default()
    }

    /// Every chat waiting on the operator, in the order they were opened. Which chats those
    /// are is [`Chat::needs_you`]'s rule.
    pub fn needs_you(&self) -> Vec<u32> {
        self.chats
            .iter()
            .filter(|(_, tracked)| tracked.chat.needs_you())
            .map(|(number, _)| *number)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_answered_in_the_window_puts_the_turn_back_to_running_and_still_asking() {
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);
        chat.reported(Event::Notification);
        assert_eq!((chat.state(), chat.asking()), (State::Waiting, true));
        assert!(chat.needs_you());

        assert!(chat.answered());

        assert_eq!((chat.state(), chat.asking()), (State::Running, true));
        assert!(!chat.needs_you());
        // Once is all: a second answer, or one to a chat asking nothing, moves nothing.
        assert!(!chat.answered());
        let mut idle = Chat::new();
        idle.reported(Event::UserPromptSubmit);
        idle.reported(Event::Stop);
        assert!(!idle.answered());
        // The turn's end clears it, as it does any prompt.
        chat.reported(Event::Stop);
        assert_eq!((chat.state(), chat.asking()), (State::Waiting, false));
    }

    #[test]
    fn a_chat_nothing_has_reported_is_unknown_and_wants_nobody() {
        // Spec decision 3: a harness with no state hook shows `unknown`, labelled as unknown.
        // A chat that has reported nothing is indistinguishable from one that never will.
        let chat = Chat::new();

        assert_eq!(chat.state(), State::Unknown);
        assert!(!chat.needs_you());
    }

    use crate::harness::model::{Ask, Began, Item, Said, Session, Turn};

    fn said_started(started: Started) -> Detail {
        Detail {
            started,
            ..Detail::default()
        }
    }

    fn said_ending(ending: Ending) -> Detail {
        Detail {
            ending,
            ..Detail::default()
        }
    }

    #[test]
    fn a_session_start_is_a_session_beginning_unless_it_was_a_compaction() {
        assert_eq!(
            Event::SessionStart.said(said_started(Started::Freshly)),
            Said::Session(Session::Began(Began::Fresh))
        );
        // Nothing said which is what a session start means when nothing says otherwise.
        assert_eq!(
            Event::SessionStart.said(said_started(Started::Unsaid)),
            Said::Session(Session::Began(Began::Fresh))
        );
        assert_eq!(
            Event::SessionStart.said(said_started(Started::Cleared)),
            Said::Session(Session::Began(Began::Cleared))
        );
        assert_eq!(
            Event::SessionStart.said(said_started(Started::Compacted)),
            Said::Session(Session::Compacted)
        );
    }

    #[test]
    fn a_session_end_for_a_clear_is_not_the_session_ending() {
        // Measured on claude 2.1.276: `/clear` fires `SessionEnd(reason=clear)` and then
        // `SessionStart(source=clear)` from the same process.
        assert_eq!(
            Event::SessionEnd.said(said_ending(Ending::Cleared)),
            Said::Session(Session::ClearedAway)
        );
        assert_eq!(
            Event::SessionEnd.said(said_ending(Ending::ForGood)),
            Said::Session(Session::Ended)
        );
    }

    #[test]
    fn a_prompt_begins_a_turn_and_stop_ends_it() {
        assert_eq!(
            Event::UserPromptSubmit.said(Detail::default()),
            Said::Turn(Turn::Began)
        );
        assert_eq!(Event::Stop.said(Detail::default()), Said::Turn(Turn::Ended));
    }

    #[test]
    fn a_notification_is_an_ask_with_no_options_said() {
        assert_eq!(
            Event::Notification.said(Detail::default()),
            Said::Ask(Ask::default())
        );
    }

    #[test]
    fn a_child_agent_finishing_is_an_item_of_the_turn_and_not_its_end() {
        assert_eq!(
            Event::SubagentStop.said(Detail::default()),
            Said::Item(Item::ChildEnded)
        );
    }

    #[test]
    fn a_chat_moves_on_what_its_harness_said_in_the_neutral_model() {
        // ADR 0073: the board reads the neutral model, so a level-3 adapter that says a turn
        // began moves a chat exactly as a level-2 `UserPromptSubmit` does.
        use crate::harness::model::{Ask, Said, Turn};
        let mut chat = Chat::new();

        assert!(chat.heard(&Said::Turn(Turn::Began)));
        assert_eq!(chat.state(), State::Running);
        assert!(chat.heard(&Said::Ask(Ask::default())));
        assert!(chat.asking());
        assert!(chat.needs_you());
    }

    #[test]
    fn a_plan_or_usage_moves_no_chat() {
        // Neither is a state the board draws, at any level.
        use crate::harness::model::{Plan, Said, Turn, Usage};
        let mut chat = Chat::new();
        chat.heard(&Said::Turn(Turn::Began));

        assert!(!chat.heard(&Said::Plan(Plan::default())));
        assert!(!chat.heard(&Said::Usage(Usage::default())));
        assert_eq!(chat.state(), State::Running);
    }

    #[test]
    fn a_prompt_puts_a_chat_in_a_turn() {
        // Measured on claude 2.1.276: UserPromptSubmit is the turn's rising edge, and the
        // Python charter's `hooks.userpromptsubmit` calls it exactly that.
        let mut chat = Chat::new();

        assert!(chat.reported(Event::UserPromptSubmit));
        assert_eq!(chat.state(), State::Running);
        assert!(!chat.needs_you());
    }

    #[test]
    fn a_turn_that_ends_hands_the_chat_back_to_you() {
        // `Stop` is the falling edge. The agent has nothing more to do, so the next move is
        // the operator's — which is what the queue is for.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);

        assert!(chat.reported(Event::Stop));
        assert_eq!(chat.state(), State::Waiting);
        assert!(chat.needs_you());
    }

    #[test]
    fn a_harness_that_asks_something_mid_turn_needs_you_at_once() {
        // Claude Code fires `Notification` for a permission prompt: the turn has not ended,
        // but it cannot go on without an answer.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);

        assert!(chat.reported(Event::Notification));
        assert_eq!(chat.state(), State::Waiting);
        assert!(chat.needs_you());
    }

    #[test]
    fn answering_the_question_puts_the_chat_back_in_its_turn() {
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);
        chat.reported(Event::Notification);

        // The operator answered and the harness carried on: the next thing it fires is the
        // turn's own end, and until then it is running again.
        assert!(chat.reported(Event::UserPromptSubmit));
        assert_eq!(chat.state(), State::Running);
        assert!(!chat.needs_you());
    }

    #[test]
    fn a_turn_that_ends_after_a_question_still_needs_you() {
        // The chat was already `Waiting` from the `Notification`, so the STATE does not
        // change — but the reason did, and the queue must still hold it.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);
        chat.reported(Event::Notification);

        chat.reported(Event::Stop);
        assert_eq!(chat.state(), State::Waiting);
        assert!(chat.needs_you());
    }

    #[test]
    fn a_chat_that_has_never_run_is_waiting_for_a_prompt_but_asks_for_nobody() {
        // A fresh chat, and every chat M1.7 puts back at a launch, is waiting on the
        // operator in the plainest sense — it wants a first prompt. It has not ASKED for
        // anything, and a launch that dropped twenty chats into the queue would empty the
        // queue of its meaning.
        let mut chat = Chat::new();

        assert!(chat.reported(Event::SessionStart));
        assert_eq!(chat.state(), State::Waiting);
        assert!(!chat.needs_you());
    }

    #[test]
    fn a_compaction_in_the_middle_of_a_turn_does_not_say_the_turn_is_over() {
        // **A defect an independent review found.** `SessionStart` is not only a launch:
        // Claude Code fires it for `clear` and `compact` too, and auto-compact fires mid
        // turn. The chat used to drop to `waiting` while the agent was still working, so the
        // sidebar said idle and the queue was wrong until the next event.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);

        assert!(!chat.reported_from(Event::SessionStart, from(Started::Compacted)));

        assert_eq!(chat.state(), State::Running);
        assert!(!chat.needs_you());
    }

    #[test]
    fn what_a_session_start_was_for_is_read_from_the_word_the_harness_used() {
        // A surviving mutant: `"clear"` mapped to `Compacted` passed everything, because the
        // behaviour tests construct `Started` directly and nothing tested the parsing. The two
        // words mean opposite things, so getting them the wrong way round is silent.
        assert_eq!(Started::of(Some("startup")), Started::Freshly);
        assert_eq!(Started::of(Some("resume")), Started::Freshly);
        assert_eq!(Started::of(Some("clear")), Started::Cleared);
        assert_eq!(Started::of(Some("compact")), Started::Compacted);
        // A harness that names none, or one charter has not measured. The plain meaning.
        assert_eq!(Started::of(None), Started::Unsaid);
        assert_eq!(Started::of(Some("something-new")), Started::Unsaid);
        assert!(Started::of(Some("clear")).began_a_session());
        assert!(!Started::of(Some("compact")).began_a_session());
    }

    #[test]
    fn clearing_the_conversation_does_end_the_turn() {
        // The other half, and the reason the payload's `source` is carried rather than
        // guessed at: `/clear` fires the same event as a compaction and means the opposite.
        // A rule of "never over a running turn" got this one wrong, and a rule of "always"
        // got the compaction wrong.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);

        assert!(chat.reported_from(Event::SessionStart, from(Started::Cleared)));

        assert_eq!(chat.state(), State::Waiting);
    }

    #[test]
    fn a_session_start_that_says_nothing_is_a_session_beginning() {
        // A harness that names no `source` means the plain thing. (Not Codex: it was believed
        // to name none, and measured sending `startup` — #27.)
        let mut chat = Chat::new();

        assert!(chat.reported_from(Event::SessionStart, from(Started::Unsaid)));

        assert_eq!(chat.state(), State::Waiting);
        assert!(!chat.needs_you());
    }

    #[test]
    fn a_turn_that_ends_needs_you_even_if_its_prompt_was_never_seen() {
        // The app can miss an event — a hook that could not reach it, or a harness attached
        // part-way. A `Stop` still means the turn is over and the next move is the
        // operator's, so it queues on its own rather than on a prompt the app remembers.
        let mut chat = Chat::new();
        chat.reported(Event::SessionStart);

        assert!(chat.reported(Event::Stop));
        assert_eq!(chat.state(), State::Waiting);
        assert!(chat.needs_you());
    }

    #[test]
    fn a_sub_agent_finishing_does_not_end_the_turn_that_dispatched_it() {
        // `charter/hooks.py:stop` says why this is not the same event: a fan-out would blink
        // the chat out of `running` several times over, in the middle of work still going.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);

        assert!(!chat.reported(Event::SubagentStop));
        assert_eq!(chat.state(), State::Running);
        assert!(!chat.needs_you());
    }

    #[test]
    fn a_session_that_ends_is_done_and_wants_nothing() {
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);
        chat.reported(Event::Stop);

        assert!(chat.reported(Event::SessionEnd));
        assert_eq!(chat.state(), State::Done);
        assert!(!chat.needs_you(), "an ended chat cannot be attended to");
    }

    #[test]
    fn a_program_that_exits_non_zero_failed() {
        // No hook reports this, and none can: the process is gone. The exit status is the
        // program telling the app directly, which is not the harness OUTPUT that ADR 0018
        // forbids reading.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);

        assert!(chat.exited(Some(1)));
        assert_eq!(chat.state(), State::Failed);
        assert!(!chat.needs_you());
    }

    #[test]
    fn a_program_that_exits_cleanly_is_done_even_with_no_hook_behind_it() {
        // A harness that carries no state hook at all is `unknown` for its whole life, and
        // then its program exits. "Done" is the honest word for that, and it is the one
        // thing such a chat can ever say.
        let mut chat = Chat::new();

        assert_eq!(chat.state(), State::Unknown);
        assert!(chat.exited(Some(0)));
        assert_eq!(chat.state(), State::Done);
    }

    #[test]
    fn a_program_killed_by_a_signal_failed() {
        // No code at all is what a signal leaves behind. It is not a clean end.
        let mut chat = Chat::new();

        assert!(chat.exited(None));
        assert_eq!(chat.state(), State::Failed);
    }

    #[test]
    fn a_hook_that_arrives_after_the_program_is_gone_changes_nothing() {
        // Hooks run as the harness's children and can outlive it by a moment. A `Stop` that
        // lands after the exit must not resurrect a dead chat into the queue.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);
        chat.exited(Some(1));

        assert!(!chat.reported(Event::Stop));
        assert_eq!(chat.state(), State::Failed);
        assert!(!chat.needs_you());
    }

    #[test]
    fn every_event_survives_the_word_that_names_it() {
        for event in [
            Event::SessionStart,
            Event::UserPromptSubmit,
            Event::Notification,
            Event::SubagentStop,
            Event::Stop,
            Event::SessionEnd,
        ] {
            assert_eq!(Event::parse(event.word()), Some(event));
        }
    }

    #[test]
    fn the_words_are_the_ones_the_python_charter_s_hooks_json_already_uses() {
        // `hooks/hooks.json` in the charter plugin calls `charter hook sessionstart`,
        // `userpromptsubmit` and `stop`. The Rust binary takes the same words, so a plugin
        // pointed at it needs no new spelling.
        assert_eq!(Event::parse("sessionstart"), Some(Event::SessionStart));
        assert_eq!(
            Event::parse("userpromptsubmit"),
            Some(Event::UserPromptSubmit)
        );
        assert_eq!(Event::parse("stop"), Some(Event::Stop));
        assert_eq!(Event::parse("notification"), Some(Event::Notification));
        assert_eq!(Event::parse("subagentstop"), Some(Event::SubagentStop));
        assert_eq!(Event::parse("sessionend"), Some(Event::SessionEnd));
    }

    #[test]
    fn a_word_that_names_no_state_event_is_refused() {
        // `pretooluse` is the GUARD's, answered by the Python charter and not by this one.
        // Taking the word here would be answering less than the guard does.
        for word in [
            "pretooluse",
            "posttooluse",
            "",
            "SessionStart",
            "sessionstar",
        ] {
            assert_eq!(
                Event::parse(word),
                None,
                "{word:?} was taken as a state event"
            );
        }
    }

    use crate::harness::Harness;

    /// A `SessionStart` that says what it was for.
    fn from(started: Started) -> Detail {
        Detail {
            started,
            ..Detail::default()
        }
    }
    use crate::hookwire::Conversation;

    /// A conversation id both implementations accept (`SessionId`).
    const A: &str = "11111111-2222-4333-8444-555555555555";
    const B: &str = "22222222-3333-4444-8555-666666666666";
    const NESTED: &str = "99999999-9999-4999-8999-999999999999";

    /// A report from a harness that names a pid — Claude Code.
    fn report(chat: u32, event: Event, conversation: Option<&str>) -> crate::hookwire::Report {
        from_pid(chat, event, conversation, CLAUDE)
    }

    /// A chat running Claude Code, under the conversation charter chose for it.
    fn claude_chat(board: &mut Board, chat: u32, conversation: Option<&str>) {
        board.opened(
            chat,
            Some(Harness::ClaudeCode),
            conversation.map(str::to_owned),
        );
    }

    /// The pid one harness process keeps for its whole life. A second one is a second pid.
    const CLAUDE: u32 = 4242;

    fn from_pid(
        chat: u32,
        event: Event,
        conversation: Option<&str>,
        pid: u32,
    ) -> crate::hookwire::Report {
        crate::hookwire::Report {
            agent: None,
            chat,
            event,
            conversation: match conversation {
                Some(said) => Conversation::Named(said.to_owned()),
                None => Conversation::Unknown,
            },
            pid: Some(pid),
            detail: Detail::default(),
        }
    }

    /// A report from a harness that names no pid — Codex, and anything not Claude Code.
    fn unsigned(chat: u32, event: Event, conversation: Option<&str>) -> crate::hookwire::Report {
        crate::hookwire::Report {
            agent: None,
            chat,
            event,
            conversation: match conversation {
                Some(said) => Conversation::Named(said.to_owned()),
                None => Conversation::Unknown,
            },
            pid: None,
            detail: Detail::default(),
        }
    }

    #[test]
    fn the_board_moves_the_chat_a_report_names() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));

        assert!(board.reported(&report(7, Event::UserPromptSubmit, Some(A))));

        assert_eq!(board.state(7), State::Running);
        assert_eq!(board.needs_you(), Vec::<u32>::new());
    }

    #[test]
    fn a_harness_nested_inside_a_chat_cannot_move_the_chat_it_runs_in() {
        // ADR 0024, C5: a `claude` started inside the chat's own shell inherits
        // `CHARTER_CHAT` and reports a new id **from its own `CLAUDE_PID`**. Its `Stop`
        // would otherwise mark the outer chat as finished while it is still working.
        //
        // The pid is what makes this test about nesting. An earlier version wrote the nested
        // report with the chat's OWN pid, which is not a nested harness at all — it is
        // `/clear` (C6), and the two are not the same event.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(!board.reported(&from_pid(7, Event::Stop, Some(NESTED), CLAUDE + 1)));

        assert_eq!(board.state(7), State::Running);
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_harness_that_names_its_own_conversation_is_adopted_at_its_first_report() {
        // Codex has no flag to choose an id (openai/codex#14482), so charter learns it from
        // the first report and holds the chat to it from then on (ADR 0024, decision 2).
        let mut board = Board::new();
        board.opened(7, Some(Harness::Codex), None);

        assert!(board.reported(&unsigned(7, Event::UserPromptSubmit, Some(A))));
        assert!(!board.reported(&unsigned(7, Event::Stop, Some(NESTED))));

        assert_eq!(board.state(7), State::Running);
    }

    /// A `SessionEnd` that says what it was for.
    fn ending(chat: u32, ending: Ending) -> crate::hookwire::Report {
        crate::hookwire::Report {
            agent: None,
            chat,
            event: Event::SessionEnd,
            conversation: Conversation::Named(A.to_owned()),
            pid: Some(CLAUDE),
            detail: Detail {
                ending,
                ..Detail::default()
            },
        }
    }

    #[test]
    fn clearing_a_conversation_does_not_end_the_chat() {
        // **Measured on claude 2.1.276, not assumed.** Typing `/clear` fires
        // `SessionEnd(reason=clear)` on the old conversation and then
        // `SessionStart(source=clear)` on the new one, from the same process — in that order.
        //
        // A `SessionEnd` that ended the chat would therefore end every cleared chat, and the
        // `/clear` following built one commit earlier could never run: the chat would read
        // `done` until its process exited. That is the same defect a review found in the
        // identity rule, reached through a different door, and the review predicted this one
        // from the `reason` field's existence.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Stop, Some(A)));

        // The measured sequence, in the measured order.
        board.reported(&ending(7, Ending::Cleared));
        assert_ne!(board.state(7), State::Done, "the chat was ended by a clear");

        let started = crate::hookwire::Report {
            detail: Detail {
                started: Started::Cleared,
                ..Detail::default()
            },
            ..report(7, Event::SessionStart, Some(B))
        };
        board.reported(&started);

        // And it is still heard afterwards, which is the whole point.
        assert!(board.reported(&report(7, Event::UserPromptSubmit, Some(B))));
        assert_eq!(board.state(7), State::Running);
        assert!(board.reported(&report(7, Event::Stop, Some(B))));
        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn a_session_that_really_ends_still_says_so() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(board.reported(&ending(7, Ending::ForGood)));

        assert_eq!(board.state(7), State::Done);
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_session_end_does_not_freeze_a_chat_that_goes_on_working() {
        // `SessionEnd` used to latch the chat shut. Besides making `/clear` unreachable, that
        // let anything in the chat's own process tree freeze a WORKING chat at `done` with one
        // forged report. Only an exit — which comes from the operating system and cannot be
        // forged — closes a chat now.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&ending(7, Ending::ForGood));
        assert_eq!(board.state(7), State::Done);

        assert!(board.reported(&report(7, Event::UserPromptSubmit, Some(A))));
        assert_eq!(board.state(7), State::Running);
    }

    #[test]
    fn a_hook_that_lands_after_the_program_is_gone_still_changes_nothing() {
        // What the latch was actually for, and it is the exit that does it — the one signal
        // in all of this that no report can imitate.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.exited(7, Some(1));

        assert!(!board.reported(&report(7, Event::Stop, Some(A))));
        assert!(!board.reported(&ending(7, Ending::Cleared)));

        assert_eq!(board.state(7), State::Failed);
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn what_a_session_end_was_for_is_read_from_the_word_the_harness_used() {
        assert_eq!(Ending::of(Some("clear")), Ending::Cleared);
        // Every other value is the session really ending. `other` is what a `-p` run gives.
        assert_eq!(Ending::of(Some("other")), Ending::ForGood);
        assert_eq!(Ending::of(Some("exit")), Ending::ForGood);
        assert_eq!(Ending::of(Some("logout")), Ending::ForGood);
        assert_eq!(Ending::of(None), Ending::ForGood);
    }

    #[test]
    fn a_chat_that_was_cleared_keeps_reporting() {
        // **A defect an independent review found, kept as a test.** ADR 0024, C6: `/clear`
        // reports a NEW conversation id from the SAME pid. An earlier version of this file
        // carried no pid, so it could not tell that from a nested harness (C5) and refused
        // both — a chat the operator cleared then showed `running` for the rest of its life,
        // never entering the queue and never notifying again.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Stop, Some(A)));

        // `/clear`: a new conversation, the same harness process. It moves the chat's link
        // and nothing a reader sees, so it is the turn AFTER it that proves the chat is
        // still being heard.
        board.reported(&report(7, Event::SessionStart, Some(B)));

        assert!(board.reported(&report(7, Event::UserPromptSubmit, Some(B))));
        assert_eq!(board.state(7), State::Running);
        assert!(board.reported(&report(7, Event::Stop, Some(B))));
        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn a_nested_harness_is_told_from_a_clear_by_the_pid_and_only_by_the_pid() {
        // Both report a conversation the chat has not seen. The pid is the whole difference:
        // the same one is the operator clearing, a different one is a `claude` started
        // inside the chat's own shell (ADR 0024, C5 against C6).
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(
            !board.reported(&from_pid(7, Event::Stop, Some(NESTED), CLAUDE + 1)),
            "a report from another process moved the chat"
        );
        assert_eq!(board.state(7), State::Running);

        assert!(board.reported(&report(7, Event::Stop, Some(B))));
        assert_eq!(board.state(7), State::Waiting);
    }

    #[test]
    fn the_conversation_a_chat_holds_follows_a_clear_and_never_a_nested_harness() {
        // What the app's usage gauge is keyed on: Claude Code files a turn under its CURRENT
        // session id, so the gauge has to follow `/clear` (C6) — and must not follow a
        // `claude` running inside the chat (C5), whose turns are not this chat's.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        assert_eq!(board.conversation(7), Some(A), "the id charter chose");

        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&from_pid(7, Event::Stop, Some(NESTED), CLAUDE + 1));
        assert_eq!(
            board.conversation(7),
            Some(A),
            "a nested harness was followed"
        );

        board.reported(&report(7, Event::Stop, Some(B)));
        assert_eq!(
            board.conversation(7),
            Some(B),
            "the chat's own /clear was not followed"
        );

        assert_eq!(
            board.conversation(8),
            None,
            "a chat the board does not have"
        );
    }

    #[test]
    fn a_report_of_a_conversation_charter_did_not_choose_is_refused_before_adoption() {
        // `charter/hooks.py:_record_harness_report`: before adoption, a report must be of
        // the id this start chose. Otherwise the first thing to speak — which may be a
        // nested harness racing the real one — takes the chat's identity for good.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));

        assert!(!board.reported(&from_pid(7, Event::Stop, Some(NESTED), 999)));
        assert_eq!(board.state(7), State::Unknown);

        assert!(board.reported(&report(7, Event::Stop, Some(A))));
    }

    #[test]
    fn a_report_naming_no_conversation_is_refused_once_the_chat_has_adopted_one() {
        // A payload that disagreed with its environment names no conversation
        // (`hookwire::conversation`), and that is exactly what a nested harness produces.
        // Before adoption there is nothing to check it against; afterwards there is.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(!board.reported(&from_pid(7, Event::Stop, None, CLAUDE + 1)));
        assert_eq!(board.state(7), State::Running);
    }

    #[test]
    fn a_payload_charter_could_not_read_still_counts_once_the_process_is_known() {
        // **A regression an earlier fix introduced, kept as a test.** Requiring the payload
        // and the environment to agree was right; dropping the report when the payload could
        // not be read at all was not. The commonest cause is charter's own read deadline, and
        // the cost landed on the honest harness: its `Stop` was dropped and the chat showed
        // `running` for ever.
        //
        // Once the process is adopted the pid says whose report this is, whatever charter
        // managed to read of the payload.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(board.reported(&report(7, Event::Stop, None)));

        assert_eq!(board.state(7), State::Waiting);
        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn a_payload_charter_could_not_read_adopts_nothing() {
        // Before adoption there is no process to check it against, so it cannot be the one
        // report an adoption gets (`charter/hooks.py`: "a malformed report must not use up
        // the one report a start adopts").
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));

        assert!(!board.reported(&report(7, Event::UserPromptSubmit, None)));

        assert_eq!(board.state(7), State::Unknown);
    }

    /// A report whose payload contradicted its environment — a nested harness that charter
    /// could read both halves of.
    fn contradicting(chat: u32, event: Event, pid: u32) -> crate::hookwire::Report {
        crate::hookwire::Report {
            agent: None,
            chat,
            event,
            conversation: Conversation::Contradicted,
            pid: Some(pid),
            detail: Detail::default(),
        }
    }

    /// A report in a dialect charter does not read — opencode's `sessionID`, say.
    fn foreign(chat: u32, event: Event, pid: Option<u32>) -> crate::hookwire::Report {
        crate::hookwire::Report {
            agent: None,
            chat,
            event,
            conversation: Conversation::Foreign,
            pid,
            detail: Detail::default(),
        }
    }

    #[test]
    fn a_report_that_contradicts_its_own_environment_moves_nothing() {
        // A surviving mutant: deleting this refusal passed every test, because `Contradicted`
        // was only ever asserted where it is CONSTRUCTED. The whole C5 chain rested on it.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        // Even from the adopted process: a payload that disagrees with its own environment is
        // not something to act on whoever is holding the file descriptor.
        assert!(!board.reported(&contradicting(7, Event::Stop, CLAUDE)));
        assert!(!board.reported(&contradicting(7, Event::Stop, CLAUDE + 1)));

        assert_eq!(board.state(7), State::Running);
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_harness_speaking_a_dialect_charter_cannot_read_cannot_move_an_adopted_chat() {
        // **A defect a review reproduced.** A nested `opencode` inherits `CLAUDE_PID` from the
        // chat it runs in and names its conversation under `sessionID`, which charter does not
        // read — so the pid matched, the conversation was "unknown", and its `Stop` marked the
        // outer Claude chat as waiting in the middle of that chat's turn.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(!board.reported(&foreign(7, Event::Stop, Some(CLAUDE))));

        assert_eq!(board.state(7), State::Running);
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_program_charter_has_not_measured_gets_the_narrowest_rule_and_not_a_refusal() {
        // `Harness::of_command` cannot tell a plain shell from a harness charter has not
        // measured yet, so refusing one refuses the other — and a harness that genuinely
        // reports its state would read `unknown` for ever. The scenario tests' own fake
        // harness is exactly this case, and went dark when it was tried the other way.
        let mut board = Board::new();
        board.opened(7, None, None);

        assert!(board.reported(&unsigned(7, Event::UserPromptSubmit, Some(A))));
        assert_eq!(board.state(7), State::Running);

        // The narrowest rule: a report naming a process is not from a harness that names
        // none, and a later different conversation is not this chat's.
        assert!(!board.reported(&report(7, Event::Stop, Some(A))));
        assert!(!board.reported(&unsigned(7, Event::Stop, Some(B))));
        assert!(board.reported(&unsigned(7, Event::Stop, Some(A))));
        assert_eq!(board.state(7), State::Waiting);
    }

    #[test]
    fn a_claude_running_inside_a_codex_chat_cannot_take_it() {
        // **A defect a review found.** A Codex chat has no conversation charter chose, so a
        // `claude` started in its shell had nothing to contradict it — it adopted the chat
        // AND its pid, and the real Codex was refused for the rest of the chat's life.
        //
        // Which rule applies is the CHAT's harness, never the report's shape. Python refuses
        // this one layer earlier, by the chat's recorded `CHARTER_HARNESS`, and its docstring
        // names this exact case.
        let mut board = Board::new();
        board.opened(7, Some(Harness::Codex), None);

        assert!(!board.reported(&from_pid(7, Event::SessionStart, Some(NESTED), 5150)));

        // And the real Codex, which names no process, is still heard.
        assert!(board.reported(&unsigned(7, Event::UserPromptSubmit, Some(A))));
        assert_eq!(board.state(7), State::Running);
    }

    #[test]
    fn a_malformed_conversation_id_cannot_use_up_a_chat_s_one_adoption() {
        // `charter/hooks.py:_record_harness_report` validates the id FIRST and says why.
        // Anything in the chat's shell can write `{"session_id":""}`.
        let mut board = Board::new();
        board.opened(7, Some(Harness::Codex), None);

        for bad in ["", "-r", "a b", "a/../b", "x".repeat(129).as_str()] {
            assert!(
                !board.reported(&unsigned(7, Event::SessionStart, Some(bad))),
                "{bad:?} was adopted as a conversation"
            );
        }

        assert!(board.reported(&unsigned(7, Event::UserPromptSubmit, Some(A))));
    }

    #[test]
    fn a_report_for_a_chat_the_app_does_not_have_moves_nothing() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.closed(7);

        assert!(!board.reported(&report(7, Event::Stop, Some(A))));
        assert_eq!(board.state(7), State::Unknown);
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn the_queue_holds_every_chat_that_asked_for_you_in_the_order_they_opened() {
        let mut board = Board::new();
        for chat in [3, 1, 2] {
            claude_chat(&mut board, chat, Some(A));
            board.reported(&report(chat, Event::UserPromptSubmit, Some(A)));
        }
        board.reported(&report(3, Event::Stop, Some(A)));
        board.reported(&report(1, Event::Notification, Some(A)));

        assert_eq!(board.needs_you(), vec![1, 3]);
    }

    #[test]
    fn a_relaunch_that_puts_twenty_chats_back_puts_none_of_them_in_the_queue() {
        // Every chat M1.7 brings back fires `SessionStart`, and each is `waiting` in the
        // plainest sense: it wants a first prompt. None has ASKED for anybody, and a launch
        // that filled the queue would empty the queue of its meaning. This is the one place
        // the state and the queue must disagree.
        let mut board = Board::new();
        for chat in 0..20 {
            claude_chat(&mut board, chat, Some(A));
            board.reported(&report(chat, Event::SessionStart, Some(A)));
            assert_eq!(board.state(chat), State::Waiting);
        }

        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_chat_the_operator_answers_leaves_the_queue() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Stop, Some(A)));
        assert_eq!(board.needs_you(), vec![7]);

        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_chat_whose_program_died_leaves_the_queue() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Notification, Some(A)));
        assert_eq!(board.needs_you(), vec![7]);

        assert!(board.exited(7, Some(1)));

        assert_eq!(board.state(7), State::Failed);
        assert!(board.needs_you().is_empty());
    }

    // ----- ignoring a chat that asked (charter-app#248) -----

    #[test]
    fn an_ignored_chat_leaves_the_queue_and_is_still_waiting() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Stop, Some(A)));
        assert_eq!(board.needs_you(), vec![7]);

        assert!(board.ignored(7), "ignoring a chat that asked is a change");

        assert!(board.needs_you().is_empty());
        assert_eq!(board.state(7), State::Waiting);
    }

    #[test]
    fn an_ignored_chat_that_stops_again_asks_again() {
        // "Until the chat asks again": the next turn's end is a new request, and it counts.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::Stop, Some(A)));
        board.ignored(7);

        assert!(
            board.reported(&report(7, Event::Stop, Some(A))),
            "a new stop on a chat still waiting was answered as no change"
        );

        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn an_ignored_chat_that_asks_mid_turn_asks_again() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::Notification, Some(A)));
        board.ignored(7);

        assert!(board.reported(&report(7, Event::Notification, Some(A))));

        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn ignoring_a_chat_that_asked_for_nothing_changes_nothing() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(!board.ignored(7));
        assert!(!board.ignored(9), "a chat the board does not have");
        assert_eq!(board.state(7), State::Running);
    }

    #[test]
    fn ignoring_one_chat_leaves_the_others_asking() {
        let mut board = Board::new();
        claude_chat(&mut board, 1, Some(A));
        claude_chat(&mut board, 2, Some(A));
        board.reported(&report(1, Event::Stop, Some(A)));
        board.reported(&report(2, Event::Stop, Some(A)));

        board.ignored(1);

        assert_eq!(board.needs_you(), vec![2]);
    }

    #[test]
    fn ignoring_a_chat_is_not_the_chat_doing_something() {
        // The strip's overflow menu is in last-activity order (ADR 0039), and the operator
        // dismissing a request is not activity of the chat's.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::Stop, Some(A)));
        let before = board.moved_at(7);

        board.ignored(7);

        assert_eq!(board.moved_at(7), before);
    }

    // ----- asking, and how many turns a chat has had (Smart close, ADR 0064) -----

    #[test]
    fn a_notification_in_the_middle_of_a_turn_is_the_chat_asking_until_the_turn_ends() {
        // A permission or a question: the turn cannot go on without an answer, and a prompt
        // sent now would land in the question rather than after it.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);
        assert!(!chat.asking());

        chat.reported(Event::Notification);
        assert!(chat.asking());

        chat.reported(Event::Stop);
        assert!(
            !chat.asking(),
            "the turn ended, so nothing is asked any more"
        );
    }

    #[test]
    fn a_notification_after_the_turn_ended_is_a_nudge_and_not_a_question() {
        // Claude Code nudges a chat left idle after its turn with a `Notification` of its own.
        // The turn is over and nothing is being asked: the chat is only waiting.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);
        chat.reported(Event::Stop);

        chat.reported(Event::Notification);

        assert!(!chat.asking());
        assert!(chat.needs_you());
    }

    #[test]
    fn a_question_is_answered_by_the_next_prompt_a_new_session_or_the_program_ending() {
        for ends in [
            |chat: &mut Chat| {
                chat.reported(Event::UserPromptSubmit);
            },
            |chat: &mut Chat| {
                chat.reported(Event::SessionStart);
            },
            |chat: &mut Chat| {
                chat.exited(Some(0));
            },
        ] {
            let mut chat = Chat::new();
            chat.reported(Event::UserPromptSubmit);
            chat.reported(Event::Notification);

            ends(&mut chat);

            assert!(!chat.asking());
        }
    }

    #[test]
    fn a_chat_counts_the_prompts_that_started_its_turns() {
        let mut chat = Chat::new();
        assert_eq!(chat.turns(), 0);

        chat.reported(Event::SessionStart);
        chat.reported(Event::UserPromptSubmit);
        chat.reported(Event::Stop);
        chat.reported(Event::UserPromptSubmit);

        assert_eq!(chat.turns(), 2);
    }

    #[test]
    fn a_question_is_not_a_second_move_of_a_chat_already_waiting() {
        // The flag is not in what a reader is shown: a `Stop` after a question moves nothing
        // on the board, so the window is not told twice and no second notification fires.
        let mut chat = Chat::new();
        chat.reported(Event::UserPromptSubmit);
        chat.reported(Event::Notification);

        assert!(!chat.reported(Event::Stop));
        assert!(!chat.asking());
    }

    #[test]
    fn the_board_says_whether_a_chat_is_asking_and_how_many_turns_it_has_had() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Notification, Some(A)));

        assert!(board.asking(7));
        assert_eq!(board.turns(7), 1);
        assert!(!board.asking(9), "a chat the board does not have");
        assert_eq!(board.turns(9), 0);
    }

    #[test]
    fn a_nested_harness_s_prompt_is_not_a_turn_of_the_chat_it_runs_in() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::SessionStart, Some(A)));

        board.reported(&from_pid(7, Event::UserPromptSubmit, Some(NESTED), 9999));

        assert_eq!(board.turns(7), 0);
    }

    // ----- when a chat last moved (ADR 0039) -----

    #[test]
    fn the_chat_that_moved_last_has_the_highest_count() {
        // The whole of what the overflow menu asks: an order over the chats, newest first.
        let mut board = Board::new();
        claude_chat(&mut board, 1, Some(A));
        claude_chat(&mut board, 2, Some(A));

        board.reported(&report(1, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(2, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(1, Event::Stop, Some(A)));

        assert!(
            board.moved_at(1) > board.moved_at(2),
            "chat 1 moved last and reads {} against {}",
            board.moved_at(1),
            board.moved_at(2)
        );
    }

    #[test]
    fn opening_a_chat_is_the_first_thing_that_moves_it() {
        // Otherwise every chat put back at a launch reads 0 and the menu's order is the
        // strip's, which is the thing it exists not to be. The chats also have to differ
        // from each other: they were put back one after another.
        let mut board = Board::new();

        claude_chat(&mut board, 1, Some(A));
        claude_chat(&mut board, 2, Some(A));

        assert!(board.moved_at(1) > 0);
        assert!(board.moved_at(2) > board.moved_at(1));
    }

    #[test]
    fn a_report_that_changes_nothing_a_reader_sees_does_not_move_the_chat() {
        // The stamp rides `chat-moved`, which is pushed only when a reader would see a
        // difference. A stamp that advanced here would be one no window could ever learn,
        // and making every hook an event is the cost ADR 0039 names as the one not to pay.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        let running = board.moved_at(7);

        assert!(!board.reported(&report(7, Event::UserPromptSubmit, Some(A))));

        assert_eq!(board.moved_at(7), running);
    }

    #[test]
    fn a_report_that_is_not_this_chats_harness_does_not_move_the_chat() {
        // ADR 0024's C5, asked of the stamp: a `claude` running inside chat 7's own shell
        // must not be able to float that chat to the top of the menu either.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        let running = board.moved_at(7);

        assert!(!board.reported(&from_pid(7, Event::Stop, Some(NESTED), CLAUDE + 1)));

        assert_eq!(board.moved_at(7), running);
    }

    #[test]
    fn a_program_that_exits_moves_its_chat() {
        // An exit is the one move no hook reports, and a chat that just failed is exactly
        // what an operator is looking for in the menu.
        let mut board = Board::new();
        claude_chat(&mut board, 1, Some(A));
        claude_chat(&mut board, 2, Some(A));
        board.reported(&report(2, Event::UserPromptSubmit, Some(A)));

        assert!(board.exited(1, Some(1)));

        assert!(board.moved_at(1) > board.moved_at(2));
    }

    #[test]
    fn a_move_on_one_board_counts_past_every_earlier_move_on_another() {
        // Each project has its own board, and the project strip's show-more menu orders the
        // projects by their newest move (ADR 0054, charter#401). A count per board would put
        // a project that moved fifty times an hour ago above one that moved once just now.
        let mut busy = Board::new();
        let mut quiet = Board::new();
        claude_chat(&mut busy, 1, Some(A));
        claude_chat(&mut quiet, 1, Some(A));
        for _ in 0..50 {
            busy.reported(&report(1, Event::UserPromptSubmit, Some(A)));
            busy.reported(&report(1, Event::Stop, Some(A)));
        }

        quiet.reported(&report(1, Event::UserPromptSubmit, Some(A)));

        assert!(
            quiet.moved_at(1) > busy.moved_at(1),
            "the quiet project moved last and reads {} against {}",
            quiet.moved_at(1),
            busy.moved_at(1)
        );
    }

    #[test]
    fn a_chat_the_board_does_not_have_moved_at_nothing() {
        let board = Board::new();

        assert_eq!(board.moved_at(7), 0);
    }

    // ----- a refused commit (SQ-16) ----------------------------------------------------------

    #[test]
    fn a_refused_commit_puts_the_chat_in_the_queue_with_what_it_was_refused_for() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(board.commit_refused(7, "commit refused in app: a.py:2  an email address  ad**"));

        assert_eq!(board.state(7), State::Running, "the turn goes on");
        assert_eq!(board.needs_you(), vec![7]);
        assert_eq!(
            board.refusals(7),
            vec!["commit refused in app: a.py:2  an email address  ad**".to_owned()]
        );
    }

    #[test]
    fn the_next_prompt_or_an_ignore_takes_a_refused_commit_away() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.commit_refused(7, "one");
        assert!(board.reported(&report(7, Event::UserPromptSubmit, Some(A))));
        assert!(board.refusals(7).is_empty());
        assert!(board.needs_you().is_empty());

        board.commit_refused(7, "two");
        assert!(board.ignored(7));
        assert!(board.refusals(7).is_empty());
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_refused_commit_in_a_chat_whose_program_has_gone_changes_nothing() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.exited(7, Some(0));
        assert!(!board.commit_refused(7, "one"));
        assert!(!board.commit_refused(8, "one"));
        assert!(board.refusals(7).is_empty());
    }

    // ----- a report back (charter-app#259) --------------------------------------------------

    #[test]
    fn a_report_back_is_the_asking_chat_s_to_read_and_no_needs_you_item() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(board.reported_back(7, "drop commons"));

        assert_eq!(board.state(7), State::Running, "nothing is typed into it");
        assert!(
            board.needs_you().is_empty(),
            "a report goes to the chat that asked, not to the person"
        );
        assert_eq!(board.reports(7), vec!["drop commons".to_owned()]);
    }

    #[test]
    fn a_chat_already_in_the_queue_says_who_reported_back_to_it() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::Stop, Some(A)));

        board.reported_back(7, "drop commons");

        assert_eq!(board.needs_you(), vec![7], "for its own turn's end");
        assert_eq!(board.reports(7), vec!["drop commons".to_owned()]);
    }

    // ----- a chat it started was stopped (#1448) ----------------------------------------------

    #[test]
    fn a_stopped_child_is_said_on_the_chat_that_started_it_and_is_no_needs_you_item() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(board.stopped_below(7, "drop commons"));

        assert_eq!(board.stopped_of(7), vec!["drop commons".to_owned()]);
        assert!(board.reports(7).is_empty(), "it reported nothing");
        assert!(board.needs_you().is_empty());
        assert_eq!(board.state(7), State::Running);
    }

    #[test]
    fn a_stopped_child_is_forgotten_at_the_next_prompt_an_ignore_or_the_end() {
        for ends in [
            |board: &mut Board| {
                board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
            },
            |board: &mut Board| {
                board.ignored(7);
            },
            |board: &mut Board| {
                board.exited(7, Some(0));
            },
        ] {
            let mut board = Board::new();
            claude_chat(&mut board, 7, Some(A));
            board.stopped_below(7, "drop commons");

            ends(&mut board);

            assert!(board.stopped_of(7).is_empty());
        }
        let mut gone = Board::new();
        assert!(!gone.stopped_below(7, "drop commons"), "not on the board");
    }

    // ----- what the app found a chat needs the person for (#1448) -----------------------------

    fn undelivered() -> Need {
        Need::ReportUndelivered {
            asker: "steward 3".to_owned(),
        }
    }

    #[test]
    fn a_report_with_nowhere_to_go_is_a_needs_you_item_on_the_chat_that_wrote_it() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(board.needs(7, undelivered()));

        assert_eq!(board.needs_you(), vec![7]);
        assert_eq!(board.needs_of(7), vec![undelivered()]);
        assert_eq!(board.state(7), State::Running, "its state is its own");
        assert!(!board.needs(7, undelivered()), "said once");
        assert_eq!(board.needs_of(7).len(), 1);
    }

    #[test]
    fn what_a_chat_needs_the_person_for_goes_at_its_next_prompt_an_ignore_or_its_end() {
        for ends in [
            |board: &mut Board| {
                board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
            },
            |board: &mut Board| {
                board.ignored(7);
            },
            |board: &mut Board| {
                board.exited(7, Some(0));
            },
        ] {
            let mut board = Board::new();
            claude_chat(&mut board, 7, Some(A));
            board.needs(7, undelivered());

            ends(&mut board);

            assert!(board.needs_of(7).is_empty());
            assert!(board.needs_you().is_empty());
        }
    }

    #[test]
    fn nothing_is_needed_of_the_person_for_a_chat_that_is_gone() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.exited(7, Some(0));

        assert!(!board.needs(7, undelivered()));
        assert!(!board.needs(8, undelivered()), "not on the board");
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_chat_that_reported_to_its_asker_ends_its_turn_without_needing_the_person() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_to_its_asker(7);

        board.reported(&report(7, Event::Stop, Some(A)));

        assert_eq!(board.state(7), State::Waiting);
        assert!(
            board.needs_you().is_empty(),
            "it waits on the chat that asked, not on the person"
        );
        // Claude Code's nudge of a chat left idle is not it asking either.
        board.reported(&report(7, Event::Notification, Some(A)));
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_chat_that_reported_still_needs_the_person_for_a_question_and_for_its_next_turn() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_to_its_asker(7);

        // A permission prompt in the same turn, after the report.
        board.reported(&report(7, Event::Notification, Some(A)));
        assert_eq!(board.needs_you(), vec![7], "a question for the person");

        // The person prompts it again: new work, whose end is theirs to read.
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Stop, Some(A)));
        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn a_report_with_nowhere_to_go_outlasts_the_turn_that_wrote_it() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.needs(7, undelivered());

        board.reported(&report(7, Event::Stop, Some(A)));

        assert_eq!(board.needs_you(), vec![7]);
        assert_eq!(board.needs_of(7), vec![undelivered()]);
    }

    #[test]
    fn the_next_prompt_reads_the_reports_and_takes_the_item_away() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::Stop, Some(A)));
        board.reported_back(7, "drop commons");
        board.reported_back(7, "retry hooks");

        assert!(board.reported(&report(7, Event::UserPromptSubmit, Some(A))));

        assert!(board.reports(7).is_empty());
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn ignoring_a_reported_back_item_takes_it_out_of_the_queue() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::Stop, Some(A)));
        board.reported_back(7, "drop commons");

        assert!(board.ignored(7));

        assert!(board.needs_you().is_empty());
        assert!(board.reports(7).is_empty());
        assert_eq!(
            board.state(7),
            State::Waiting,
            "the chat itself is untouched"
        );
    }

    #[test]
    fn a_report_back_to_a_chat_the_board_does_not_have_changes_nothing() {
        let mut board = Board::new();

        assert!(!board.reported_back(7, "drop commons"));
        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_report_back_to_a_chat_whose_program_has_gone_changes_nothing() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.exited(7, Some(0));

        assert!(!board.reported_back(7, "drop commons"));
        assert!(board.reports(7).is_empty());
    }

    #[test]
    fn a_report_back_is_a_move() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        claude_chat(&mut board, 8, Some(A));
        let before = board.moved_at(7);

        board.reported_back(7, "drop commons");

        assert!(board.moved_at(7) > before);
        assert!(board.moved_at(7) > board.moved_at(8));
    }

    // ----- child agents (FD-18, W8) ---------------------------------------------------------

    /// A report from sub-agent `agent` of a Claude Code chat: its own process and conversation,
    /// as Claude Code sends a sub-agent's hooks.
    fn from_agent(chat: u32, event: Event, agent: &str) -> crate::hookwire::Report {
        crate::hookwire::Report {
            agent: Some(agent.to_owned()),
            ..report(chat, event, Some(A))
        }
    }

    fn children_of(board: &Board, chat: u32) -> Vec<(String, State)> {
        board
            .children(chat)
            .into_iter()
            .map(|child| (child.agent, child.state))
            .collect()
    }

    #[test]
    fn a_claude_code_sub_agent_shows_under_its_chat_and_completes_at_its_subagent_stop() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(board.reported(&from_agent(7, Event::Notification, "a1")));
        assert_eq!(children_of(&board, 7), [("a1".to_owned(), State::Running)]);

        assert!(board.reported(&from_agent(7, Event::SubagentStop, "a1")));
        assert_eq!(children_of(&board, 7), [("a1".to_owned(), State::Done)]);
        assert_eq!(board.state(7), State::Waiting, "its ask was the chat's");
    }

    #[test]
    fn a_child_s_turn_is_not_its_chat_s() {
        // ADR 0076 §6: no child move changes the chat's own state. A child that ends its own
        // turn has not ended the turn that dispatched it.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        board.reported(&from_agent(7, Event::Stop, "a1"));
        board.reported(&from_agent(7, Event::SessionStart, "a1"));

        assert_eq!(board.state(7), State::Running);
        assert!(board.needs_you().is_empty());
        assert_eq!(children_of(&board, 7), [("a1".to_owned(), State::Running)]);
    }

    #[test]
    fn a_child_reporting_another_conversation_does_not_move_its_chat_onto_it() {
        // A sub-agent's report names a conversation, and a different one is not the chat's
        // `/clear`: the chat stays where it is, and keeps its children.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.child_heard(7, "a1");

        board.reported(&crate::hookwire::Report {
            agent: Some("a2".to_owned()),
            ..report(7, Event::Notification, Some(B))
        });

        assert_eq!(board.conversation(7), Some(A));
        assert_eq!(
            children_of(&board, 7),
            [
                ("a1".to_owned(), State::Running),
                ("a2".to_owned(), State::Running)
            ]
        );
    }

    #[test]
    fn a_report_naming_an_agent_id_no_harness_would_send_is_no_child_and_still_the_chat_s_ask() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        let huge = "x".repeat(70 * 1024);

        assert!(!board.child_heard(7, &huge));
        board.reported(&from_agent(7, Event::Stop, &huge));
        assert_eq!(board.state(7), State::Running, "still a child's turn");
        assert!(board.reported(&from_agent(7, Event::Notification, &huge)));

        assert_eq!(board.state(7), State::Waiting);
        assert!(board.children(7).is_empty());
    }

    #[test]
    fn a_codex_child_shows_under_its_chat_from_its_first_tool_call() {
        // Codex is armed with no `SubagentStop`, and its child's first hook charter hears is a
        // tool call carrying `agent_id`.
        let mut board = Board::new();
        board.opened(3, Some(Harness::Codex), None);
        board.reported(&unsigned(3, Event::UserPromptSubmit, Some(A)));

        assert!(board.child_heard(3, "thread-2"));
        assert!(!board.child_heard(3, "thread-2"));

        assert_eq!(
            children_of(&board, 3),
            [("thread-2".to_owned(), State::Running)]
        );
        assert_eq!(board.state(3), State::Running);
    }

    #[test]
    fn stopping_the_parent_ends_every_child_still_working_with_it() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&from_agent(7, Event::Notification, "a1"));
        board.child_heard(7, "a2");
        board.reported(&from_agent(7, Event::SubagentStop, "a2"));

        // A stop signals the group, and a signal leaves no code.
        assert!(board.exited(7, None));

        assert_eq!(
            children_of(&board, 7),
            [
                ("a1".to_owned(), State::Failed),
                ("a2".to_owned(), State::Done)
            ]
        );
        assert!(
            !board.child_heard(7, "a3"),
            "a chat that ended spawns nothing"
        );
        assert!(!board.reported(&from_agent(7, Event::SubagentStop, "a1")));
    }

    #[test]
    fn a_sub_agent_of_a_harness_nested_in_the_chat_is_not_the_chat_s_child() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        let nested = crate::hookwire::Report {
            agent: Some("n1".to_owned()),
            ..from_pid(7, Event::Notification, Some(NESTED), 9999)
        };

        assert!(!board.reported(&nested));

        assert!(board.children(7).is_empty());
    }

    #[test]
    fn a_clear_shows_none_of_the_old_conversation_s_children() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.child_heard(7, "a1");

        board.reported(&crate::hookwire::Report {
            detail: from(Started::Cleared),
            ..report(7, Event::SessionStart, Some(B))
        });

        assert!(board.children(7).is_empty());
    }

    #[test]
    fn a_child_moving_is_a_move_of_its_chat() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        claude_chat(&mut board, 8, Some(A));
        let before = board.moved_at(7);

        board.child_heard(7, "a1");

        assert!(board.moved_at(7) > before);
        assert!(board.moved_at(7) > board.moved_at(8));
    }

    #[test]
    fn a_chat_the_board_does_not_have_has_no_children() {
        let mut board = Board::new();

        assert!(!board.child_heard(4, "a1"));
        assert!(board.children(4).is_empty());
    }

    // ----- a chat waiting on its tasks, or on its asker (#1491) -----

    /// A chat with `tasks` tasks below it still working, asking or owing a report.
    fn with_tasks(tasks: u32) -> Waits {
        Waits {
            tasks,
            its_asker: false,
            line_coming: false,
        }
    }

    /// A task paused on a question to the chat that dispatched it.
    const ASKS_ITS_ASKER: Waits = Waits {
        tasks: 0,
        its_asker: true,
        line_coming: false,
    };

    /// A chat a line of purlis's own is about to start a turn of.
    const A_LINE_IS_COMING: Waits = Waits {
        tasks: 0,
        its_asker: false,
        line_coming: true,
    };

    #[test]
    fn a_session_whose_turn_ends_while_a_task_below_it_works_is_not_in_the_queue() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert!(
            board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(2)),
            "its state still moved: the turn is over"
        );

        assert_eq!(board.state(7), State::Waiting);
        assert!(board.needs_you().is_empty(), "it waits on its tasks");
        assert!(board.held(7));
    }

    #[test]
    fn a_task_any_depth_below_holds_the_session_as_one_right_under_it_does() {
        // The board is told a count and holds no lineage: a task two dispatches down is one
        // task below, as the app's records count it, and it holds the end the same.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(1));

        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn the_nudge_of_a_session_left_idle_while_its_tasks_work_raises_nothing_either() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(1));

        board.reported_while(&report(7, Event::Notification, Some(A)), with_tasks(1));

        assert!(board.needs_you().is_empty());
        assert!(!board.asking(7), "a nudge after the turn asks nothing");
    }

    #[test]
    fn a_session_is_an_item_once_its_tasks_have_reported_and_it_has_then_stopped() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(1));
        // The last report lands: the session is typed a line, reads it in a turn of its own,
        // and stops with nothing below it.
        board.reported_back(7, "talk");
        assert!(
            board.needs_you().is_empty(),
            "a report landing is no item: the session has it to read"
        );
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        assert!(!board.held(7));

        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(0));

        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn a_session_nothing_will_prompt_is_an_item_once_what_it_waited_on_is_over() {
        // The app typed it no line (its harness takes none, or the person had keys in its
        // pane), so no turn of its own follows the last report: the app says it is at rest.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(1));

        assert!(board.rested(7), "the held end is the item now");

        assert_eq!(board.needs_you(), vec![7]);
        assert!(!board.held(7));
        assert!(!board.rested(7), "and it is said once");
    }

    #[test]
    fn a_chat_no_end_was_held_for_is_not_put_in_the_queue_by_being_at_rest() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        claude_chat(&mut board, 8, Some(B));
        // 7 is mid-turn; 8 was held and has begun a turn since.
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&from_pid(8, Event::UserPromptSubmit, Some(B), 77));
        board.reported_while(&from_pid(8, Event::Stop, Some(B), 77), with_tasks(1));
        board.reported(&from_pid(8, Event::UserPromptSubmit, Some(B), 77));

        assert!(!board.rested(7));
        assert!(!board.rested(8));
        assert!(!board.rested(99), "a chat the board does not have");

        assert!(board.needs_you().is_empty());
    }

    #[test]
    fn a_task_paused_on_a_question_to_its_asker_is_not_in_the_queue() {
        // Its turn ended because it asked the chat that dispatched it, which is who can
        // answer: the #1484 leftover, where its row read "needs you".
        let mut board = Board::new();
        claude_chat(&mut board, 9, Some(A));
        board.reported(&report(9, Event::UserPromptSubmit, Some(A)));

        board.reported_while(&report(9, Event::Stop, Some(A)), ASKS_ITS_ASKER);

        assert_eq!(board.state(9), State::Waiting);
        assert!(board.needs_you().is_empty());
        // Answered, with no line typed into it: it is the person's to prompt.
        assert!(board.rested(9));
        assert_eq!(board.needs_you(), vec![9]);
    }

    #[test]
    fn a_real_prompt_is_an_item_whatever_the_tasks_below_are_doing() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        // A permission or a question, mid-turn.
        board.reported_while(&report(7, Event::Notification, Some(A)), with_tasks(3));
        assert_eq!(board.needs_you(), vec![7], "a prompt mid-turn");
        assert!(board.asking(7));

        // A reason the app found, on a session held for its tasks.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(3));
        board.needs(
            7,
            Need::ReportUndelivered {
                asker: "steward 1".to_owned(),
            },
        );
        assert_eq!(board.needs_you(), vec![7], "a Notice's reason");

        // A refused commit.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(3));
        board.commit_refused(7, "a key in app.env");
        assert_eq!(board.needs_you(), vec![7], "a refused commit");

        // A task paused on its asker that shows the person a prompt of its own.
        let mut board = Board::new();
        claude_chat(&mut board, 9, Some(A));
        board.reported(&report(9, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(9, Event::Notification, Some(A)), ASKS_ITS_ASKER);
        assert_eq!(board.needs_you(), vec![9], "its own prompt wins");
    }

    #[test]
    fn what_a_session_already_asked_for_stays_when_its_turn_then_ends_held() {
        // It asked mid-turn, was answered, and its turn then ended with a task at work: the
        // item it had is the prompt's, which is still the person's until they prompt it.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(7, Event::Notification, Some(A)), with_tasks(1));

        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(1));

        assert_eq!(
            board.needs_you(),
            vec![7],
            "what it already asked for stays"
        );
    }

    #[test]
    fn a_held_session_the_person_ignored_stays_out_until_it_asks_again() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::Stop, Some(A)));
        assert_eq!(board.needs_you(), vec![7]);
        board.ignored(7);

        // Ignored, then a task is at work below it at its next stop: still not an item.
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(1));
        assert!(board.needs_you().is_empty());

        // And the ignore's own rule holds: the next end with nothing below asks again.
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Stop, Some(A)));
        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn a_task_that_failed_is_an_item_on_the_chat_that_asked_and_one_that_is_done_is_not() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported_while(&report(7, Event::Stop, Some(A)), with_tasks(2));

        // Done: the report is the session's to read, and no item.
        assert!(board.reported_back(7, "talk"));
        assert!(board.needs_you().is_empty());

        // Failed: an item on the session, though another task still works below it.
        assert!(board.task_failed(
            7,
            FailedTask::new(
                "r-build",
                "build",
                HowFailed::Failed,
                "the forge refused the push"
            )
        ));
        assert_eq!(board.needs_you(), vec![7]);
        assert_eq!(
            board.failed_tasks(7),
            vec![FailedTask {
                id: "r-build".to_owned(),
                chat: None,
                task: "build".to_owned(),
                how: HowFailed::Failed,
                why: "the forge refused the push".to_owned(),
            }]
        );
        assert!(
            !board.task_failed(
                7,
                FailedTask::new(
                    "r-build",
                    "build",
                    HowFailed::Failed,
                    "the forge refused the push"
                )
            ),
            "said once"
        );
    }

    #[test]
    fn a_failed_task_s_item_outlasts_the_turn_that_reads_its_report() {
        // The session is typed a line when the report lands, so its next prompt is moments
        // away: a reason that prompt cleared would be gone before anybody saw it.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.task_failed(
            7,
            FailedTask::new("r-build", "build", HowFailed::Unreported, ""),
        );

        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        assert_eq!(board.state(7), State::Running);
        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn a_failed_task_s_item_goes_when_the_person_looks_or_clears_its_row_or_ignores_it() {
        let failed = || FailedTask::new("r-build", "build", HowFailed::Unreported, "");
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        // Looked at: the item goes, and the session goes on working.
        board.task_failed(7, failed());
        assert!(board.failures_seen(7));
        assert!(board.needs_you().is_empty());
        assert!(board.failed_tasks(7).is_empty());
        assert!(!board.failures_seen(7), "nothing left to look at");

        // Its row cleared: that task's item, and no other's.
        board.task_failed(7, failed());
        board.task_failed(
            7,
            FailedTask::new(
                "r-lint",
                "lint",
                HowFailed::DidNotStart,
                "no profile runs it",
            ),
        );
        assert!(board.failure_cleared(7, "r-build"));
        assert_eq!(
            board.failed_tasks(7),
            vec![FailedTask::new(
                "r-lint",
                "lint",
                HowFailed::DidNotStart,
                "no profile runs it"
            )]
        );
        assert_eq!(board.needs_you(), vec![7]);
        assert!(!board.failure_cleared(7, "r-build"));

        // Ignored: the person's dismissal takes it, as it takes every reason.
        assert!(board.ignored(7));
        assert!(board.needs_you().is_empty());

        // And the end of the session's program takes it too.
        board.task_failed(7, failed());
        board.exited(7, Some(0));
        assert!(board.needs_you().is_empty());
        assert!(
            !board.task_failed(7, failed()),
            "an ended chat asks for nobody"
        );
    }

    #[test]
    fn looking_at_a_failure_leaves_what_else_the_session_needs_the_person_for() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        board.reported(&report(7, Event::Stop, Some(A)));
        board.task_failed(
            7,
            FailedTask::new("r-build", "build", HowFailed::Failed, ""),
        );

        board.failures_seen(7);

        assert_eq!(board.needs_you(), vec![7], "its own end of turn still asks");
    }

    #[test]
    fn why_a_task_failed_is_said_in_a_few_words() {
        assert_eq!(
            FailedTask::in_a_few_words("\n  the  build\tbroke \nand more"),
            "the build broke"
        );
        assert_eq!(FailedTask::in_a_few_words(""), "");
        let long = "word ".repeat(40);
        let cut = FailedTask::in_a_few_words(&long);
        assert!(cut.chars().count() <= FailedTask::WHY_AT_MOST, "{cut}");
        assert!(cut.ends_with('…'), "{cut}");
        // Cut by characters, never inside one.
        let wide = "é".repeat(200);
        assert_eq!(
            FailedTask::in_a_few_words(&wide).chars().count(),
            FailedTask::WHY_AT_MOST
        );
    }

    #[test]
    fn a_turn_that_ends_with_a_line_about_to_start_the_next_is_held_and_released_if_none_does() {
        // A report landed mid-turn: purlis types its line as this turn ends, so the end is
        // not the person's. Where the line is never taken, the app says so and it is.
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));

        board.reported_while(&report(7, Event::Stop, Some(A)), A_LINE_IS_COMING);

        assert!(board.needs_you().is_empty());
        assert!(board.held(7));
        assert!(board.rested(7));
        assert_eq!(board.needs_you(), vec![7]);
    }

    #[test]
    fn two_tasks_of_one_name_that_fail_are_two_failures_and_each_is_cleared_by_itself() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        assert!(board.task_failed(7, FailedTask::new("r-1", "check", HowFailed::Failed, "")));
        assert!(board.task_failed(7, FailedTask::new("r-2", "check", HowFailed::Failed, "")));
        assert!(
            !board.task_failed(
                7,
                FailedTask::new("r-2", "check", HowFailed::Failed, "again")
            ),
            "one record is one failure"
        );

        assert!(board.failure_cleared(7, "r-1"));

        assert_eq!(
            board.failed_tasks(7),
            vec![FailedTask::new("r-2", "check", HowFailed::Failed, "")]
        );
        assert_eq!(board.needs_you(), vec![7], "the other still flags it");
    }

    #[test]
    fn a_move_raises_something_only_on_the_edge_into_the_queue_or_a_new_reason() {
        let mut board = Board::new();
        claude_chat(&mut board, 7, Some(A));
        board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        let mut was = board.standing(7);
        let mut moved = |board: &mut Board, act: &dyn Fn(&mut Board)| {
            act(board);
            let now = board.standing(7);
            let rose = raised(was, now);
            was = now;
            rose
        };

        // A task fails: the rising edge.
        assert!(moved(&mut board, &|board| {
            board.task_failed(7, FailedTask::new("r-1", "build", HowFailed::Failed, ""));
        }));
        // The chat reads the report in a turn of its own, and a helper of its comes and goes:
        // it moved, the item stands, and nothing was raised.
        assert!(!moved(&mut board, &|board| {
            board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        }));
        assert!(!moved(&mut board, &|board| {
            board.child_heard(7, "a1");
        }));
        assert!(!moved(&mut board, &|board| {
            board.reported(&report(7, Event::Stop, Some(A)));
        }));
        // A second task fails while the first still stands: a new reason.
        assert!(moved(&mut board, &|board| {
            board.task_failed(7, FailedTask::new("r-2", "lint", HowFailed::Failed, ""));
        }));
        // Looked at, both: it is still an item for its own stop, and nothing was raised.
        assert!(!moved(&mut board, &|board| {
            board.failure_cleared(7, "r-1");
            board.failure_cleared(7, "r-2");
        }));
        // Its next prompt takes it out; the stop after that is the edge again.
        assert!(!moved(&mut board, &|board| {
            board.reported(&report(7, Event::UserPromptSubmit, Some(A)));
        }));
        assert!(moved(&mut board, &|board| {
            board.reported(&report(7, Event::Stop, Some(A)));
        }));
    }
}
