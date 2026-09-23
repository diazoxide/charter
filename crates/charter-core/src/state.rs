//! What a chat's state is, and the only thing allowed to move it.
//!
//! Nothing here reads a harness's output (ADR 0018, spec decision 3). A state changes for
//! exactly two reasons: a harness hook reported an [`Event`], or the session's own program
//! exited — which is the process telling the app, not a screen charter read.

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
    pub fn began_a_session(self) -> bool {
        match self {
            Self::Freshly | Self::Cleared | Self::Unsaid => true,
            // The agent is still working, and the chat is still whatever it was.
            Self::Compacted => false,
        }
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
}

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
}

impl Chat {
    /// A chat nothing has reported yet.
    pub fn new() -> Self {
        Self {
            state: State::Unknown,
            needs_you: false,
            ended: false,
        }
    }

    /// The state as it stands.
    pub fn state(&self) -> State {
        self.state
    }

    /// Whether this chat belongs in the "needs you" queue.
    pub fn needs_you(&self) -> bool {
        self.needs_you
    }

    /// A hook reported `event`. Answers whether anything a reader can see changed.
    pub fn reported(&mut self, event: Event) -> bool {
        self.reported_from(event, Detail::default())
    }

    /// The same, with what the harness said about the event beyond its name.
    pub fn reported_from(&mut self, event: Event, detail: Detail) -> bool {
        let Detail { started, ending } = detail;
        if self.ended {
            return false;
        }
        let was = (self.state, self.needs_you);
        match event {
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
            Event::SessionStart if !started.began_a_session() => {}
            Event::SessionStart => {
                self.state = State::Waiting;
            }
            Event::UserPromptSubmit => {
                self.state = State::Running;
                self.needs_you = false;
            }
            // The turn has not ended, but it cannot go on without an answer.
            Event::Notification => {
                self.state = State::Waiting;
                self.needs_you = true;
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
            Event::Stop => {
                self.state = State::Waiting;
                self.needs_you = true;
            }
            // Emphatically not `Stop`: a dispatched sub-agent finishing does not end the
            // turn that dispatched it, and a fan-out would blink the chat out of `running`
            // several times over (`charter/hooks.py:stop`).
            Event::SubagentStop => {}
            // **`/clear` ends a CONVERSATION, not the session** — measured on claude 2.1.276,
            // where typing it fires `SessionEnd(reason=clear)` and then
            // `SessionStart(source=clear)` from the same process. The `SessionStart` that
            // follows says what the chat is now.
            Event::SessionEnd if ending == Ending::Cleared => {}
            Event::SessionEnd => {
                self.state = State::Done;
                self.needs_you = false;
            }
        }
        was != (self.state, self.needs_you)
    }

    /// The session's program exited. Answers whether anything a reader can see changed.
    ///
    /// No hook reports this and none can — the process is gone. An exit status is the
    /// program telling the app directly, which is not the harness OUTPUT that ADR 0018
    /// forbids reading.
    pub fn exited(&mut self, code: Option<i32>) -> bool {
        let was = (self.state, self.needs_you);
        // No code at all is what a signal leaves behind, and that is not a clean end.
        self.state = if code == Some(0) {
            State::Done
        } else {
            State::Failed
        };
        self.needs_you = false;
        self.ended = true;
        was != (self.state, self.needs_you)
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
    /// How many times anything on this board has moved. See [`Board::moved_at`].
    moves: u32,
}

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
        let Some(tracked) = self.chats.get_mut(&report.chat) else {
            // A report for a chat the app does not have: one that was closed a moment ago,
            // or a stale environment in a process that outlived it.
            return false;
        };
        if !tracked.take(report) {
            return false;
        }
        let changed = tracked.chat.reported_from(report.event, report.detail);
        self.stamp(report.chat, changed)
    }

    /// The chat's program exited. Answers whether anything a reader can see changed.
    pub fn exited(&mut self, number: u32, code: Option<i32>) -> bool {
        let changed = self
            .chats
            .get_mut(&number)
            .is_some_and(|tracked| tracked.chat.exited(code));
        self.stamp(number, changed)
    }

    /// When this chat last moved, as a count of moves on this board — bigger is more recent.
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
        self.moves = self.moves.saturating_add(1);
        self.moves
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

    /// Every chat waiting on the operator, in the order they were opened.
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
    fn a_chat_nothing_has_reported_is_unknown_and_wants_nobody() {
        // Spec decision 3: a harness with no state hook shows `unknown`, labelled as unknown.
        // A chat that has reported nothing is indistinguishable from one that never will.
        let chat = Chat::new();

        assert_eq!(chat.state(), State::Unknown);
        assert!(!chat.needs_you());
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
    fn a_chat_the_board_does_not_have_moved_at_nothing() {
        let board = Board::new();

        assert_eq!(board.moved_at(7), 0);
    }
}
