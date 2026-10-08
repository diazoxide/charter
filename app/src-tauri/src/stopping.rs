//! **Stop this chat**, and **Stop this chat and everything below it** (#1448): the person ends
//! a chat another chat started, or a whole run of them, from the window.
//!
//! **Only the person can stop a chat they did not ask it to start.** The two are one window
//! command ([`stop_chat`]), and ending a task is another ([`end_task`]); they are the only
//! callers of `press`, which is private to this file: the compiler holds that. The close dialog's "Stop them" (#1443) is the same stop
//! ([`press_below`]), begun by the window's close and by nothing else. No line on the hook
//! socket reads as a stop, so no chat can stop another, or itself, or have a brief do it. What
//! a chat has is `purlis dispatch cancel` (#1441), for a task it dispatched itself and for no
//! other chat: it ends nothing, and asks that task for its report.
//!
//! **One stop, whoever asked for it** (D-T59-j3). Stop, "Stop them" and the Close of a task
//! that had not reported end in one word to the chat that asked, written in one place
//! ([`crate::handoff::operator_stopped`]). A task being stopped is not cancelled as well: its
//! cancel stands down when the stop begins, and a cancel asked for after it is refused.
//!
//! **A chat being stopped starts no chat** ([`refuses_a_start`]), and nor does a chat below one:
//! a ticket, a handoff and a dispatch from it are refused in fixed words
//! ([`STARTS_NOTHING`]), so nothing it starts can outlive the stop. A chat it had already begun
//! to start is ended as it opens ([`sweeps`]). And it is not started again under a new number
//! while its stop is under way ([`Stopping::is_stopping`]).
//!
//! **A chat another chat started gets one short turn to write what it did.** It is sent one
//! line ([`PROMPT`], or [`TASK_PROMPT`] for a task) asking for a last report to the chat that asked, and it ends when that
//! turn does. When the line is sent is the board's answer, never the screen's, by the rule
//! Smart close sends its prompt on (ADR 0064): at once to a chat that is waiting, at the
//! turn's end to one mid-turn, and **never into a chat that is showing a prompt** — a
//! permission or a question. That one is ended as it stands, and so is a shell tab, a chat
//! whose program has ended and one purlis has heard nothing from. A chat the person started
//! themselves is ended at once: nobody is waiting on what it would write.
//!
//! **The last turn is short because it is bounded** ([`LAST_TURN`]): a chat that has not ended
//! its turn by then is ended anyway. And pressing Stop on a chat that is already stopping ends
//! it there and then, so the person is never left waiting on a chat they asked to stop.
//!
//! **The chat that asked is told the operator stopped it**, through the delivery a report uses
//! ([`crate::handoff::deliver`]): left for its next turn while it is open, kept for its
//! workspace when it is not. **That word is purlis's own, and is marked so**
//! (`purlis_core::handback::Stopped`): no report a chat sends can carry the mark, and it is
//! drawn as purlis's sentence, never as something the stopped chat said. A report the stopped
//! chat wrote in its last turn travelled the same way just before, quoted as its own. A chat
//! that is ending in the same stop is not told: it has no turn left to read it in.
//!
//! **The person ends a task in one of two ways** (#1488, V100-5), through this one engine and
//! one window command of its own ([`end_task`]):
//!
//! - **Stop and get its report** ([`Way::Report`]). A task mid-turn has that turn ended as a
//!   cancel ends it (Escape, a moment for it to stop, then the line), and one waiting is asked
//!   at once. Only where purlis may type into it at all (`dispatched::report_turn`): never at a
//!   prompt, over the person's own keys, on a harness purlis types nothing into or one it has
//!   not heard from. There it is ended as it stands. The wait is bounded twice: a line that
//!   starts no turn purlis hears ([`dispatched::A_PROMPT_IS_HEARD_WITHIN`], the cancel's own
//!   bound) and a turn that does not end ([`LAST_TURN`]) both end it with what is known.
//! - **Close now** ([`Way::Now`]). Its program is ended at once, with no turn and no report.
//!
//! **What the chat that asked is then told is one of two words of purlis's own**: stopped by
//! the person, carrying the one report the task sent once its stop was recorded, or closed by
//! the person, carrying none (`purlis_core::handback`). **The stop is the outcome from the
//! moment it is recorded**, under the lock a report is taken under: a report the task sends
//! after that, whatever it says of itself and whether or not it had been asked yet, is the
//! stop's report; one that landed before it was an ordinary report, and the task had ended by
//! itself.
//!
//! **Everything below, deepest first.** The stop of a subtree takes the chats the lineage
//! nests under the pressed one and no other. A chat's own stop begins only when every chat
//! below it has ended, so each chat's last turn is handed what the chats under it wrote, and
//! no chat ends before one it started.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use purlis_core::dispatched;
use purlis_core::state::State;

use crate::planes::{Held, PlaneId, Planes};

/// The event the window is told each step of a stop on.
pub const EVENT: &str = "chat-stop";

/// How long a stopped chat has, from the moment its stop begins, to end its last turn. A turn
/// that writes a few lines takes seconds; this is room for a slow machine and a turn that was
/// already under way, and short enough that Stop still means stop.
pub const LAST_TURN: Duration = Duration::from_secs(90);

/// What a stopped chat is sent for its last turn. It names the one command, as the handoff's
/// own ask does, and says what happens next.
pub const PROMPT: &str = "The operator stopped this chat. Start nothing new. In this one turn, \
     report what you did and what is left undone in a few lines, with `purlis handoff report \
     \"<summary>\"`. This chat ends when the turn does.";

/// What a chat being stopped, or a chat below one, is told when it asks to start a chat.
pub const STARTS_NOTHING: &str =
    "this chat is being stopped by the operator; it cannot start a chat";

/// What a restart of a chat being stopped is refused with.
pub const NOT_STARTED_AGAIN: &str = "This chat is being stopped, so it is not started again.";

/// [`PROMPT`], for a chat that was dispatched as a task: its report says how it ended, and has
/// a command of its own.
pub const TASK_PROMPT: &str = "The operator stopped this chat. Start nothing new. In this one \
     turn, report what you did and what is left undone in a few lines, with `purlis dispatch report \
     --outcome blocked \"<summary>\"` (or --outcome done or failed, whichever is true). This \
     chat ends when the turn does.";

/// The bytes the prompt is sent as, to a task when `task` and to a handoff otherwise: one
/// bracketed paste, then Enter, in one write.
pub fn sent_as(task: bool) -> String {
    let prompt = if task { TASK_PROMPT } else { PROMPT };
    format!("{}\r", crate::curation::bracketed(prompt))
}

/// One step of one chat's stop, as the window is told it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ChatStop {
    pub plane: PlaneId,
    pub session: u32,
    pub phase: StopPhase,
}

/// Where a chat's stop stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum StopPhase {
    /// It is being stopped: waiting for the chats below it, or writing its last turn.
    Stopping,
    /// It has ended, and its tab goes.
    Stopped,
}

/// Told each step of every plane's stops.
pub type Teller = Arc<dyn Fn(ChatStop) + Send + Sync + 'static>;

/// What decides whether a chat gets a last turn, and when: the app's facts about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub state: State,
    /// A permission or a question is open mid-turn (`Board::asking`).
    pub asking: bool,
    /// Prompts that started a turn, as this app heard them.
    pub turns: u32,
    /// A shell tab: no harness, no profile.
    pub shell: bool,
    /// Another chat started it, so a chat is waiting on what it writes. Not a task whose
    /// report is already settled: nothing is left for it to write.
    pub dispatched: bool,
    /// It was dispatched as a task (#1488): its turn is ended as a cancel ends one, and it is
    /// typed into only where `types` says purlis may.
    pub task: bool,
    /// purlis may type into it: its harness is one purlis types a line into, and no key of
    /// the person's has gone to its pane since its harness last spoke. Asked of a task only.
    pub types: bool,
}

/// Which of the two ways the person ends a chat (#1488).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Way {
    /// One short turn to write what it did, where it can be given one, then its end.
    Report,
    /// Its end, at once.
    Now,
}

/// When a chat being stopped is sent its last-turn prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastTurn {
    /// Now: it is waiting, and asks nothing.
    Now,
    /// When the turn it is in ends.
    AtTurnEnd,
    /// Once the turn it is in has been ended for it: a task's, as a cancel ends one.
    Interrupted,
    /// Never: it is ended as it stands.
    None,
}

/// Whether a chat with these facts gets a last turn, and when ([`LastTurn`]).
pub fn last_turn(facts: &Facts) -> LastTurn {
    if !facts.dispatched || facts.shell {
        return LastTurn::None;
    }
    // A task is typed into only where purlis may type at all (D-1441-13).
    if facts.task && !facts.types {
        return LastTurn::None;
    }
    match facts.state {
        // Showing a prompt: nothing is typed into it.
        State::Waiting if facts.asking => LastTurn::None,
        State::Waiting => LastTurn::Now,
        State::Running if facts.task => LastTurn::Interrupted,
        State::Running => LastTurn::AtTurnEnd,
        State::Unknown | State::Done | State::Failed => LastTurn::None,
    }
}

/// `root` and every chat below it, **deepest first** and `root` last, or `root` alone when
/// not `below`. `chats` is every open chat with the chat that started it. A lineage that loops
/// is cut at the first chat seen twice, so no chat is listed twice and none outside the subtree
/// is listed at all.
pub fn subtree(chats: &[(u32, Option<u32>)], root: u32, below: bool) -> Vec<u32> {
    let mut found = vec![(root, 0_usize)];
    let mut seen = HashSet::from([root]);
    let mut at = 0;
    while below && at < found.len() {
        let (parent, depth) = found[at];
        at += 1;
        let mut started: Vec<u32> = chats
            .iter()
            .filter(|(chat, by)| *by == Some(parent) && !seen.contains(chat))
            .map(|(chat, _)| *chat)
            .collect();
        started.sort_unstable();
        for chat in started {
            if seen.insert(chat) {
                found.push((chat, depth + 1));
            }
        }
    }
    // Stable, so chats at one depth stay in number order.
    found.sort_by_key(|(_, depth)| std::cmp::Reverse(*depth));
    found.into_iter().map(|(chat, _)| chat).collect()
}

/// One thing a stop asks the app to do, in the order it is to be done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// The chat's own stop has begun: tell the window, start its clock (`number` names this
    /// stop of it), and send it the prompt now when `send`.
    Begun {
        session: u32,
        number: u64,
        send: bool,
    },
    /// Send the chat its last-turn prompt: the turn it was in has ended.
    Send { session: u32 },
    /// End the turn the task is in, as the person's Escape would, and look again
    /// ([`Stops::settled`]) once it has had its moment to stop. `number` names this stop.
    Interrupt { session: u32, number: u64 },
    /// End the chat. `wrote` is whether it sent a last report. `tell` is whether the chat that
    /// asked is told: not when that chat is ending in the same stop.
    End {
        session: u32,
        wrote: bool,
        tell: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    /// Waiting for the chats below it in the same stop to end.
    Held,
    /// Waiting for the turn it is in to end, before the prompt is sent.
    Queued,
    /// Its turn was ended for it, and is having its moment to stop before the prompt is sent.
    Interrupted,
    /// The prompt was sent when the chat had had this many turns: the last turn has ended when
    /// it has had more and waits again.
    Sent { turns: u32 },
    /// A task that sent its report before it was asked: it ends when the turn it is in does.
    Wrote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    step: Step,
    number: u64,
    /// The chat that started it, where one did.
    asker: Option<u32>,
    /// The chats it started that are in the same stop and have not ended.
    below: Vec<u32>,
    /// Whether it sent a report in its last turn.
    wrote: bool,
    /// It was dispatched as a task: a report it sends at any step of its stop is the stop's.
    task: bool,
}

/// The stops under way in one plane, by chat: the whole of the deciding, with nothing that
/// touches a chat. Every method answers the [`Act`]s to carry out, in order.
#[derive(Debug, Default)]
pub struct Stops {
    chats: HashMap<u32, Entry>,
    dealt: u64,
}

impl Stops {
    /// The person pressed Stop on the chats `order` — [`subtree`]'s, deepest first. `started`
    /// answers the chat that started a chat, and `facts` what the app knows of one.
    ///
    /// **A press on chats that are all stopping already ends them now**, deepest first: the
    /// person asked twice. **A press that takes in chats not yet stopping** gives those an
    /// ordinary stop, last turn and all, and ends nothing early: "everything below" pressed on
    /// a chat that was being stopped alone. A chat of the earlier stop that has not been sent
    /// its prompt waits for the new ones below it.
    ///
    /// **`way` is how** (#1488): [`Way::Now`] ends every chat of `order` there and then,
    /// deepest first, whether or not it was stopping already.
    pub fn press(
        &mut self,
        order: &[u32],
        way: Way,
        started: impl Fn(u32) -> Option<u32>,
        facts: impl Fn(u32) -> Facts,
    ) -> Vec<Act> {
        let fresh: Vec<u32> = order
            .iter()
            .copied()
            .filter(|session| !self.chats.contains_key(session))
            .collect();
        if way == Way::Now {
            // Recorded first, so each chat's end knows whether the chat that asked for it is
            // ending in the same stop.
            for session in &fresh {
                self.dealt += 1;
                self.chats.insert(
                    *session,
                    Entry {
                        step: Step::Held,
                        number: self.dealt,
                        asker: started(*session),
                        below: Vec::new(),
                        wrote: false,
                        task: facts(*session).task,
                    },
                );
            }
        }
        if fresh.is_empty() || way == Way::Now {
            let mut acts: Vec<Act> = order
                .iter()
                .filter_map(|session| self.take(*session))
                .collect();
            acts.extend(self.advance(&facts));
            return acts;
        }
        for session in &fresh {
            self.dealt += 1;
            self.chats.insert(
                *session,
                Entry {
                    step: Step::Held,
                    number: self.dealt,
                    asker: started(*session),
                    below: Vec::new(),
                    wrote: false,
                    task: facts(*session).task,
                },
            );
        }
        for session in order {
            let below: Vec<u32> = order
                .iter()
                .copied()
                .filter(|other| other != session && started(*other) == Some(*session))
                .collect();
            let is_fresh = fresh.contains(session);
            let waits_again = below.iter().any(|one| fresh.contains(one));
            let Some(entry) = self.chats.get_mut(session) else {
                continue;
            };
            if is_fresh {
                entry.below = below;
            } else if waits_again && matches!(entry.step, Step::Held | Step::Queued) {
                // Not yet sent its prompt: it waits for the chats below it, as a chat pressed
                // with them would have. Its clock starts again when its own stop does.
                self.dealt += 1;
                entry.step = Step::Held;
                entry.number = self.dealt;
                entry.below = below;
            }
        }
        self.advance(&facts)
    }

    /// Whether chat `session` is being stopped, or is below a chat that is: `started` answers
    /// the chat that started a chat. Such a chat starts no chat.
    pub fn holds(&self, session: u32, started: impl Fn(u32) -> Option<u32>) -> bool {
        let mut at = Some(session);
        // A lineage is at most a few deep; the bound is for one that loops.
        for _ in 0..64 {
            let Some(chat) = at else { return false };
            if self.chats.contains_key(&chat) {
                return true;
            }
            at = started(chat);
        }
        false
    }

    /// Whether any chat is being stopped.
    pub fn any(&self) -> bool {
        !self.chats.is_empty()
    }

    /// Whether chat `session` itself is being stopped.
    pub fn has(&self, session: u32) -> bool {
        self.chats.contains_key(&session)
    }

    /// The board heard from chat `session`, whose facts are now `facts`: a queued prompt is
    /// sent when its turn has ended, and a chat whose last turn has ended is ended. A chat that
    /// began showing a prompt before its own was sent is ended as it stands.
    pub fn moved(&mut self, session: u32, facts: impl Fn(u32) -> Facts) -> Vec<Act> {
        let Some(step) = self.chats.get(&session).map(|entry| entry.step.clone()) else {
            return Vec::new();
        };
        let now = facts(session);
        let ready = now.state == State::Waiting && !now.asking;
        match step {
            Step::Held => Vec::new(),
            Step::Queued if ready => {
                if let Some(entry) = self.chats.get_mut(&session) {
                    entry.step = Step::Sent { turns: now.turns };
                }
                vec![Act::Send { session }]
            }
            Step::Queued if last_turn(&now) == LastTurn::None => self.end(session, &facts),
            Step::Queued => Vec::new(),
            Step::Sent { turns } if ready && now.turns > turns => self.end(session, &facts),
            Step::Sent { .. } => Vec::new(),
            // It reported before it was asked: the turn it reported in has ended, or its
            // program has.
            Step::Wrote if ready => self.end(session, &facts),
            // Showing a prompt, or gone, before its line could be sent: ended as it stands.
            Step::Wrote | Step::Interrupted if !interrupted_takes_a_line(&now) => {
                self.end(session, &facts)
            }
            Step::Wrote | Step::Interrupted => Vec::new(),
        }
    }

    /// Chat `session`'s interrupted turn has had its moment to stop, in stop `number`: it is
    /// sent its line where purlis may still type into it, and is ended as it stands where it
    /// may not. Nothing where that stop is over, or has moved on.
    pub fn settled(&mut self, session: u32, number: u64, facts: impl Fn(u32) -> Facts) -> Vec<Act> {
        let Some(entry) = self.chats.get_mut(&session) else {
            return Vec::new();
        };
        if entry.number != number || entry.step != Step::Interrupted {
            return Vec::new();
        }
        let now = facts(session);
        if interrupted_takes_a_line(&now) {
            entry.step = Step::Sent { turns: now.turns };
            vec![Act::Send { session }]
        } else {
            self.end(session, &facts)
        }
    }

    /// The line sent to chat `session` in stop `number` began no turn purlis heard, in the
    /// time a cancel's line is given: the chat is ended with what is known, so a stop never
    /// waits for good on a line that was swallowed. Nothing where a turn did begin on it.
    pub fn unheard(&mut self, session: u32, number: u64, facts: impl Fn(u32) -> Facts) -> Vec<Act> {
        let waiting = self.chats.get(&session).is_some_and(|entry| {
            entry.number == number
                && matches!(entry.step, Step::Sent { turns } if facts(session).turns <= turns)
        });
        if waiting {
            self.end(session, &facts)
        } else {
            Vec::new()
        }
    }

    /// The number of chat `session`'s stop, while it has one: what its clocks name it by.
    pub fn number(&self, session: u32) -> Option<u64> {
        self.chats.get(&session).map(|entry| entry.number)
    }

    /// Chat `session`'s clock for stop `number` ran out: it is ended, unless that stop of it is
    /// over or has not begun.
    pub fn timed_out(
        &mut self,
        session: u32,
        number: u64,
        facts: impl Fn(u32) -> Facts,
    ) -> Vec<Act> {
        let still = self
            .chats
            .get(&session)
            .is_some_and(|entry| entry.number == number && entry.step != Step::Held);
        if still {
            self.end(session, &facts)
        } else {
            Vec::new()
        }
    }

    /// Chat `session` is to be ended whatever its stop was waiting for: its prompt could not be
    /// written, or its program ended on its own. Nothing for a chat that is not being stopped.
    pub fn give_up(&mut self, session: u32, facts: impl Fn(u32) -> Facts) -> Vec<Act> {
        if !self.chats.contains_key(&session) {
            return Vec::new();
        }
        self.end(session, &facts)
    }

    /// Chat `session` is gone from the app by another road — its tab's Close. Its stop is let
    /// go of, and the chats above it no longer wait for it.
    pub fn forget(&mut self, session: u32, facts: impl Fn(u32) -> Facts) -> Vec<Act> {
        if self.take(session).is_none() {
            return Vec::new();
        }
        // Closed already: nothing is ended twice, and nobody is told it was stopped.
        self.advance(&facts)
    }

    /// **Takes the one report a stop asks for**, whatever the chat owed before: true when chat
    /// `session` is writing its last turn and had taken none, and from then it has. One call
    /// tests and takes, so two reports in flight are one report and one refusal.
    ///
    /// **A task's is taken at any step of its stop** (#1488): the stop is the outcome from the
    /// moment it is recorded, so a report the task sends before it has been asked is the
    /// stop's all the same, and it is then asked for nothing more.
    pub fn take_last_report(&mut self, session: u32) -> bool {
        match self.chats.get_mut(&session) {
            Some(entry) if matches!(entry.step, Step::Sent { .. }) && !entry.wrote => {
                entry.wrote = true;
                true
            }
            Some(entry) if entry.task && !entry.wrote => {
                entry.wrote = true;
                if matches!(entry.step, Step::Queued | Step::Interrupted) {
                    entry.step = Step::Wrote;
                }
                true
            }
            _ => false,
        }
    }

    /// Gives back the report chat `session` took and could not send: it may try once more.
    pub fn give_back_last_report(&mut self, session: u32) {
        if let Some(entry) = self.chats.get_mut(&session) {
            entry.wrote = false;
        }
    }

    /// Every chat being stopped, in number order.
    pub fn now(&self) -> Vec<u32> {
        let mut now: Vec<u32> = self.chats.keys().copied().collect();
        now.sort_unstable();
        now
    }

    /// Ends chat `session` and begins whatever was waiting for it.
    fn end(&mut self, session: u32, facts: &impl Fn(u32) -> Facts) -> Vec<Act> {
        let mut acts: Vec<Act> = self.take(session).into_iter().collect();
        acts.extend(self.advance(facts));
        acts
    }

    /// Takes chat `session` out, answering its end, and frees the chats that waited for it.
    fn take(&mut self, session: u32) -> Option<Act> {
        let entry = self.chats.remove(&session)?;
        for other in self.chats.values_mut() {
            other.below.retain(|below| *below != session);
        }
        Some(Act::End {
            session,
            wrote: entry.wrote,
            // A chat still in this stop is ending too, and reads nothing more.
            tell: entry
                .asker
                .is_none_or(|asker| !self.chats.contains_key(&asker)),
        })
    }

    /// Begins the stop of every held chat nothing below is left of, lowest number first, and
    /// again for each chat that ending one frees.
    fn advance(&mut self, facts: &impl Fn(u32) -> Facts) -> Vec<Act> {
        let mut acts = Vec::new();
        loop {
            let mut free: Vec<u32> = self
                .chats
                .iter()
                .filter(|(_, entry)| entry.step == Step::Held && entry.below.is_empty())
                .map(|(session, _)| *session)
                .collect();
            free.sort_unstable();
            let Some(session) = free.first().copied() else {
                return acts;
            };
            let now = facts(session);
            // A task that reported while it waited for the chats below it has written what
            // its stop asks for: it is asked for nothing more.
            let wrote = self.chats.get(&session).is_some_and(|entry| entry.wrote);
            match last_turn(&now) {
                LastTurn::None => acts.extend(self.take(session)),
                _ if wrote => acts.extend(self.take(session)),
                when => {
                    let send = when == LastTurn::Now;
                    if let Some(entry) = self.chats.get_mut(&session) {
                        entry.step = match when {
                            LastTurn::Now => Step::Sent { turns: now.turns },
                            LastTurn::Interrupted => Step::Interrupted,
                            _ => Step::Queued,
                        };
                        acts.push(Act::Begun {
                            session,
                            number: entry.number,
                            send,
                        });
                        if when == LastTurn::Interrupted {
                            acts.push(Act::Interrupt {
                                session,
                                number: entry.number,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Whether a task whose turn was ended for it may be sent its line now: purlis may type into
/// it, its program runs, and it shows the person no prompt. **Not whether it waits**: no
/// harness reports an interrupted turn, so the board still says it is running
/// (`dispatched::Ledger::cancel_step` sends a cancel's line by the same rule).
fn interrupted_takes_a_line(facts: &Facts) -> bool {
    facts.types && !facts.asking && matches!(facts.state, State::Running | State::Waiting)
}

/// One plane's stops, behind the lock every step of one is decided under.
#[derive(Debug, Default)]
pub struct Stopping {
    stops: Mutex<Stops>,
    /// What a stop asked for while the lock a close is made under was held: carried out when
    /// that hold is let go ([`carry_pending`]), because ending a chat takes the same lock.
    pending: Mutex<Vec<Act>>,
    /// By the chat a stop was pressed on: the tasks ended with it, below it, by name, as the
    /// word to the chat that asked names them (#1488). Kept until that chat has ended.
    named: Mutex<HashMap<u32, Vec<String>>>,
}

impl Stopping {
    fn stops(&self) -> MutexGuard<'_, Stops> {
        self.stops.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes the one report chat `session`'s stop asks for ([`Stops::take_last_report`]).
    pub fn take_last_report(&self, session: u32) -> bool {
        self.stops().take_last_report(session)
    }

    /// Gives back a report taken and not sent ([`Stops::give_back_last_report`]).
    pub fn give_back_last_report(&self, session: u32) {
        self.stops().give_back_last_report(session);
    }

    /// Whether chat `session` is being stopped: such a chat is not started again under a new
    /// number, which would take it out of its stop.
    pub fn is_stopping(&self, session: u32) -> bool {
        self.stops().has(session)
    }

    /// Every chat being stopped.
    pub fn now(&self) -> Vec<u32> {
        self.stops().now()
    }

    fn named(&self) -> MutexGuard<'_, HashMap<u32, Vec<String>>> {
        self.named.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The tasks being ended below chat `session` in the stop the person pressed on it, by
    /// name: what its word to the chat that asked names.
    pub fn ended_below(&self, session: u32) -> Vec<String> {
        self.named().get(&session).cloned().unwrap_or_default()
    }

    /// Keeps `acts` for [`carry_pending`].
    fn pend(&self, acts: Vec<Act>) {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(acts);
    }
}

/// Carries out what stops asked for under a hold of the deciding lock, now that it is let go.
/// Called by whoever held it: a close, "Stop them", a chat started again in its own place.
pub(crate) fn carry_pending(held: &Arc<Held>) {
    let acts = std::mem::take(
        &mut *held
            .stopping()
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    );
    carry_out(held, acts);
}

/// A cancel of each of `chats` stands down: the person's stop is the later word (D-T59-j3).
fn cancels_stand_down(held: &Held, chats: &[u32]) {
    let mut ledger = held.tasks().ledger();
    for chat in chats {
        ledger.stood_down(*chat);
    }
}

/// The facts [`last_turn`] decides on, for chat `session` of `held`. A chat that is not open
/// reads as ended, which ends its stop.
fn facts_of(held: &Held, session: u32) -> Facts {
    let open = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == session);
    let board = held.board().glance(session);
    match open {
        Some(open) => {
            let task = open
                .from
                .as_ref()
                .is_some_and(|from| from.mode == purlis_core::dispatchdecision::Mode::Task);
            // A task whose report is settled has nothing left to write: it sent it, or purlis
            // said in its place that it went without one.
            let settled = task
                && open
                    .from
                    .as_ref()
                    .is_some_and(|from| from.report != purlis_core::reopen::Owed::Due);
            Facts {
                state: board.state,
                asking: board.asking,
                turns: board.turns,
                shell: open.harness.is_none() && open.profile.is_none(),
                dispatched: open.from.is_some() && !settled,
                task,
                types: dispatched::told_by_a_line(held.chats().harness(session))
                    && !held.tasks().ledger().keyed(session),
            }
        }
        None => Facts {
            state: State::Done,
            asking: false,
            turns: board.turns,
            shell: false,
            dispatched: false,
            task: false,
            types: false,
        },
    }
}

/// Every open chat with the chat that started it, as `held` records them.
fn lineage_of(held: &Held) -> Vec<(u32, Option<u32>)> {
    held.chats()
        .open_now()
        .iter()
        .map(|chat| (chat.session, chat.from.as_ref().map(|from| from.chat)))
        .collect()
}

/// The chat that started `chat`, by `lineage`.
fn started_in(lineage: &[(u32, Option<u32>)], chat: u32) -> Option<u32> {
    lineage
        .iter()
        .find(|(one, _)| *one == chat)
        .and_then(|(_, by)| *by)
}

/// **The person pressed Stop on chat `session`**, alone or with everything `below` it. The
/// only way a stop begins. Private to this file, so the two window commands ([`stop_chat`],
/// [`end_task`]) are its only callers.
///
/// **The lineage is read and the stop recorded in one hold**, the one [`refuses_a_start`] and
/// [`sweeps`] take: a chat that opens is either read here, and stopped with the rest, or opens
/// after the stop is recorded, and is refused or ended as it opens.
///
/// **And under the lock a dispatch is decided and its slot reserved under** (D-T59-j4), so a
/// stop and a decision come one after the other: a dispatch decided first has its slot by the
/// time the stop is recorded, and is refused before its chat is started
/// ([`refuses_a_start`], asked again as it starts).
fn press(held: &Arc<Held>, session: u32, below: bool, way: Way) -> Result<(), String> {
    press_where(held, session, below, way, false)
}

/// What ending a chat that is not a task, by the task's own command, is refused with.
pub const NOT_A_TASK: &str = "That chat is not a task, so it is not ended this way.";

/// [`press`]; where `a_task`, only on a chat a dispatch started as a task, asked under the
/// hold the stop is recorded in: what [`end_task`] holds to "a task, and still open" by.
fn press_where(
    held: &Arc<Held>,
    session: u32,
    below: bool,
    way: Way,
    a_task: bool,
) -> Result<(), String> {
    let acts = {
        let _deciding = held.chats().deciding();
        let lineage = lineage_of(held);
        if !lineage.iter().any(|(chat, _)| *chat == session) {
            return Err("That chat is not open any more.".to_owned());
        }
        let is_a_task = held.chats().handed_from(session).is_some_and(|from| {
            from.mode == purlis_core::dispatchdecision::Mode::Task && from.chat != session
        });
        if a_task && !is_a_task {
            return Err(NOT_A_TASK.to_owned());
        }
        let order = subtree(&lineage, session, below);
        // The tasks ended with it, below it, that still owed their report: what the word to
        // the chat that asked names. Read here, while each is still open.
        let named: Vec<String> = order
            .iter()
            .filter(|chat| **chat != session)
            .filter(|chat| held.chats().owed_task_report(**chat).is_some())
            .filter_map(|chat| held.chats().shown_name(*chat))
            .map(|name| purlis_core::handback::in_purlis_s_line(&name))
            .take(purlis_core::handback::MOST_NAMED_BELOW)
            .collect();
        if is_a_task && !named.is_empty() {
            // Added to what an earlier press on it named: a task below that has ended since
            // was ended with it all the same.
            let mut all = held.stopping().named();
            let known = all.entry(session).or_default();
            for name in named {
                if !known.contains(&name) && known.len() < purlis_core::handback::MOST_NAMED_BELOW {
                    known.push(name);
                }
            }
        }
        let acts = held.stopping().stops().press(
            &order,
            way,
            |chat| started_in(&lineage, chat),
            |chat| facts_of(held, chat),
        );
        cancels_stand_down(held, &order);
        acts
    };
    carry_out(held, acts);
    Ok(())
}

/// **"Stop them"** (#1443): the person's answer as they close chat `asker`. Every chat at work
/// below it (`order`, deepest first) is stopped by the stop every chat is stopped by: one short
/// turn for a chat another chat started, then its end, and the one word to the chat that asked.
///
/// Under the caller's hold of the deciding lock, which the close of `asker` is made under too:
/// no chat below starts another between the answer and the stop, and from here on none of them
/// starts one at all ([`refuses_a_start`]). What the stop asks for is carried out when the hold
/// is let go ([`carry_pending`]).
pub(crate) fn press_below(held: &Held, order: &[u32], _deciding: &crate::handoff::Deciding<'_>) {
    if order.is_empty() {
        return;
    }
    let lineage = lineage_of(held);
    let acts = held.stopping().stops().press(
        order,
        Way::Report,
        |chat| started_in(&lineage, chat),
        |chat| facts_of(held, chat),
    );
    cancels_stand_down(held, order);
    held.stopping().pend(acts);
}

/// [`press`], for a test in another file that holds a plane open. Not in the app.
#[cfg(test)]
pub(crate) fn press_in_a_test(held: &Arc<Held>, session: u32, below: bool) -> Result<(), String> {
    press(held, session, below, Way::Report)
}

/// [`end_a_task`], for a test in another file that holds a plane open. Not in the app.
#[cfg(test)]
pub(crate) fn end_task_in_a_test(
    held: &Arc<Held>,
    session: u32,
    way: Way,
    below: bool,
) -> Result<(), String> {
    end_a_task(held, session, way, below)
}

/// A clock of chat `session`'s stop ran out, in a test that does not wait for it: the line it
/// was sent began no turn (`unheard`), or its last turn took too long.
#[cfg(test)]
pub(crate) fn clock_ran_out_in_a_test(held: &Arc<Held>, session: u32, unheard: bool) {
    let Some(number) = held.stopping().stops().number(session) else {
        return;
    };
    let acts = if unheard {
        held.stopping()
            .stops()
            .unheard(session, number, |chat| facts_of(held, chat))
    } else {
        held.stopping()
            .stops()
            .timed_out(session, number, |chat| facts_of(held, chat))
    };
    carry_out(held, acts);
}

/// The moment an interrupted turn is given has passed for chat `session`, in a test that does
/// not wait for it.
#[cfg(test)]
pub(crate) fn settled_in_a_test(held: &Arc<Held>, session: u32) {
    let Some(number) = held.stopping().stops().number(session) else {
        return;
    };
    let acts = held
        .stopping()
        .stops()
        .settled(session, number, |chat| facts_of(held, chat));
    carry_out(held, acts);
}

/// **Whether chat `chat` may start no chat**: it is being stopped, or is below a chat that is
/// (#1448). Asked as a ticket is spent on a handoff or a dispatch, before either starts
/// anything, and never as one is minted: a ticket is also what the chat's one last report is
/// sent on, so a chat in a stop is still given one (D-T59-j9).
///
/// **It is a chat's own start that is refused.** The person asking a persona from that chat's
/// tab (#1438) is not the chat starting one: they pressed Stop, and they may still ask
/// (D-T59-j4). The callers ask this of a chat's own ask only.
pub(crate) fn refuses_a_start(held: &Held, chat: u32) -> bool {
    let stops = held.stopping().stops();
    if !stops.any() {
        return false;
    }
    let lineage = lineage_of(held);
    stops.holds(chat, |one| started_in(&lineage, one))
}

/// **Chat `started` has just opened for chat `asker`**: when `asker` came under a stop while it
/// was starting, `started` is ended here and now, and the answer is true. The start is then
/// refused in [`STARTS_NOTHING`]'s words, and nothing of it outlives the stop.
pub(crate) fn sweeps(held: &Held, started: u32, asker: u32) -> bool {
    if !refuses_a_start(held, asker) {
        return false;
    }
    if let Err(why) = held.close_chat(started) {
        tracing::warn!(
            "purlis: chat {started}, started by a chat being stopped, did not end cleanly ({why})"
        );
    }
    true
}

/// A report reached the plane from chat `chat`: its stop, if it has one, moves on.
pub fn reported(held: &Arc<Held>, chat: u32) {
    let acts = held
        .stopping()
        .stops()
        .moved(chat, |chat| facts_of(held, chat));
    carry_out(held, acts);
}

/// Chat `session`'s program ended on its own while it was being stopped: it is ended as a stop
/// ends it, and the chat that asked is told.
pub fn exited(held: &Arc<Held>, session: u32) {
    let acts = held
        .stopping()
        .stops()
        .give_up(session, |chat| facts_of(held, chat));
    carry_out(held, acts);
}

/// Chat `session` was closed by another road: its stop is let go of, and whatever waited for
/// it begins once the close's hold is let go ([`carry_pending`]): a close is made under the
/// lock that ending the next chat takes.
pub fn closed(held: &Held, session: u32) {
    held.stopping().named().remove(&session);
    let acts = held
        .stopping()
        .stops()
        .forget(session, |chat| facts_of(held, chat));
    held.stopping().pend(acts);
}

/// Carries `acts` out in order, and whatever each one leads to.
fn carry_out(held: &Arc<Held>, acts: Vec<Act>) {
    let mut acts = std::collections::VecDeque::from(acts);
    while let Some(act) = acts.pop_front() {
        match act {
            Act::Begun {
                session,
                number,
                send,
            } => {
                held.tell_stop(session, StopPhase::Stopping);
                end_later(held, session, number);
                if send {
                    acts.push_front(Act::Send { session });
                }
            }
            Act::Send { session } => {
                // The command it is told is the one its report is taken by.
                let task = held
                    .chats()
                    .handed_from(session)
                    .is_some_and(|from| from.mode == purlis_core::dispatchdecision::Mode::Task);
                let sent = held
                    .chats()
                    .sessions()
                    .input(session, sent_as(task).as_bytes());
                if sent.is_err() {
                    // Nothing to write to: it is ended as it stands.
                    let more = held
                        .stopping()
                        .stops()
                        .give_up(session, |chat| facts_of(held, chat));
                    for act in more.into_iter().rev() {
                        acts.push_front(act);
                    }
                } else if task {
                    // The line starts a turn, and the task is working again from now: said
                    // to the ledger as any line purlis types is. And a line that starts no
                    // turn purlis hears ends the stop, by the bound a cancel's line has.
                    held.tasks().ledger().line_typed(session);
                    let number = held.stopping().stops().number(session);
                    if let Some(number) = number {
                        look_later(
                            held,
                            session,
                            number,
                            dispatched::A_PROMPT_IS_HEARD_WITHIN,
                            Clock::Unheard,
                        );
                    }
                }
            }
            Act::Interrupt { session, number } => {
                if let Err(why) = held
                    .chats()
                    .sessions()
                    .input(session, dispatched::INTERRUPT)
                {
                    tracing::warn!("purlis: chat {session}'s turn could not be ended ({why})");
                }
                look_later(
                    held,
                    session,
                    number,
                    dispatched::A_TURN_STOPS_WITHIN,
                    Clock::Settled,
                );
            }
            Act::End {
                session,
                wrote,
                tell,
            } => end(held, session, wrote, tell),
        }
    }
}

/// Ends chat `session`, stopped by the person: the chat that asked is told when `tell`, in
/// purlis's own marked word, then the chat is closed as its tab's Close closes it, and the
/// window takes its tab away.
fn end(held: &Held, session: u32, wrote: bool, tell: bool) {
    let below = held.stopping().named().remove(&session).unwrap_or_default();
    {
        // The one word, by the one function, under the lock a report is taken under: the
        // close that follows finds the task settled and says nothing a second time.
        let deciding = held.chats().deciding();
        crate::handoff::operator_stopped(held, session, wrote, tell, below, &deciding);
    }
    if let Err(why) = held.close_chat(session) {
        tracing::warn!("purlis: chat {session}, stopped, did not end cleanly ({why})");
    }
    held.tell_stop(session, StopPhase::Stopped);
}

/// Ends stop `number` of chat `session` when its last turn has taken too long. On a thread of
/// its own, holding the plane only weakly: a closed project is not kept open by it.
fn end_later(held: &Arc<Held>, session: u32, number: u64) {
    let weak = Arc::downgrade(held);
    let clock = std::thread::Builder::new()
        .name("purlis-chat-stop".into())
        .spawn(move || {
            std::thread::sleep(LAST_TURN);
            let Some(held) = weak.upgrade() else { return };
            let acts = held
                .stopping()
                .stops()
                .timed_out(session, number, |chat| facts_of(&held, chat));
            carry_out(&held, acts);
        });
    if let Err(why) = clock {
        // Its last turn is then bounded by nothing but the person's second press.
        tracing::warn!(
            "purlis: the clock for chat {session}'s last turn could not be started ({why}), so \
             it ends when that turn does, or when Stop is pressed on it again"
        );
    }
}

/// Which of a task's stop's two short clocks ran out ([`look_later`]).
#[derive(Clone, Copy)]
enum Clock {
    /// Its interrupted turn has had its moment to stop.
    Settled,
    /// Its line has had as long as a line is given to start a turn purlis hears.
    Unheard,
}

/// Looks at stop `number` of chat `session` again in `after`. On a thread of its own, holding
/// the plane only weakly, as [`end_later`] does.
fn look_later(held: &Arc<Held>, session: u32, number: u64, after: Duration, clock: Clock) {
    let weak = Arc::downgrade(held);
    let spawned = std::thread::Builder::new()
        .name("purlis-task-stop".into())
        .spawn(move || {
            std::thread::sleep(after);
            let Some(held) = weak.upgrade() else { return };
            let acts = {
                let mut stops = held.stopping().stops();
                match clock {
                    Clock::Settled => stops.settled(session, number, |chat| facts_of(&held, chat)),
                    Clock::Unheard => stops.unheard(session, number, |chat| facts_of(&held, chat)),
                }
            };
            carry_out(&held, acts);
        });
    if let Err(why) = spawned {
        // Its stop is still bounded by its last turn's own clock, and by a second press.
        tracing::warn!("purlis: a clock for chat {session}'s stop could not be started ({why})");
    }
}

/// What ending a task would do, as the window asks before it does it (#1488, V100-18): what
/// decides whether the person is asked anything, and which of the two ways is offered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct TaskEnding {
    /// The task, as its row names it.
    pub name: String,
    /// It is in the middle of a turn: ending it asks first. An idle task, and one that has
    /// reported, is ended without a question.
    pub working: bool,
    /// Why it cannot be given a turn to report, in one sentence for the person, where it
    /// cannot: then only Close now is offered. Nothing where it can.
    pub no_report: Option<String>,
    /// The tasks below it that are still at work, by name, deepest first: the person is asked
    /// once whether they are stopped with it or kept.
    pub below: Vec<String>,
    /// It is being stopped already.
    pub stopping: bool,
    /// Its report is settled: it reported, or purlis said in its place that it went without
    /// one. Ending it then tells the chat that asked nothing more.
    pub reported: bool,
}

/// [`TaskEnding`] for chat `session` of `held`, or why it is not a task that can be ended.
pub(crate) fn task_ending_of(held: &Held, session: u32) -> Result<TaskEnding, String> {
    let from = held
        .chats()
        .handed_from(session)
        .filter(|from| from.mode == purlis_core::dispatchdecision::Mode::Task)
        .ok_or_else(|| NOT_A_TASK.to_owned())?;
    let name = held
        .chats()
        .shown_name(session)
        .ok_or_else(|| "That chat is not open any more.".to_owned())?;
    let open = held
        .chats()
        .open_now()
        .iter()
        .any(|open| open.session == session);
    let board = held.board().glance(session);
    let seen = dispatched::Seen {
        ended: !open || matches!(board.state, State::Done | State::Failed),
        heard: board.state != State::Unknown,
        running: board.state == State::Running,
        waiting: board.state == State::Waiting,
        asking: board.asking,
        measured: dispatched::told_by_a_line(held.chats().harness(session)),
    };
    let keyed = held.tasks().ledger().keyed(session);
    let settled = from.report != purlis_core::reopen::Owed::Due;
    Ok(TaskEnding {
        working: !settled && !seen.ended && (seen.running || seen.asking),
        no_report: dispatched::report_turn(seen, keyed, from.report)
            .err()
            .map(|why| why.say(&name)),
        below: crate::handoff::running_below(held, session),
        stopping: held.stopping().is_stopping(session),
        reported: settled,
        name,
    })
}

/// **Ends task `session` the way the person chose** ([`Way`]), with the tasks at work below
/// it where `below`, or leaving those to finish where not. A chat that is not a task is
/// refused: this is a task's command, and a session is closed by its tab.
fn end_a_task(held: &Arc<Held>, session: u32, way: Way, below: bool) -> Result<(), String> {
    press_where(held, session, below, way, true)
}

/// What ending a task would do: whether it is mid-turn, whether it can be asked for a report
/// and why not where it cannot, and the tasks still at work below it.
// Its plane is a `PlaneId` the registry vouches for. **A window command and nothing else.**
#[tauri::command]
#[specta::specta]
pub fn task_ending(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<TaskEnding, String> {
    let held = planes.held(&plane)?;
    task_ending_of(&held, session)
}

/// Ends a task, the way you chose: `report` gives it one short turn to say what it did, where
/// it can be given one, and then ends it; `now` ends its program at once, with no report.
/// With `below`, the tasks still at work below it are ended the same way, deepest first;
/// without, they are left to finish. The chat that asked is told which, in purlis's own
/// words: stopped by the person, with the task's report, or closed by the person.
// Its plane is a `PlaneId` the registry vouches for. **A window command and nothing else**: no
// hook line reaches it, and no chat's own command does. On a blocking thread: ending a
// program waits for it to go.
#[tauri::command]
#[specta::specta]
pub async fn end_task(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    way: Way,
    below: bool,
) -> Result<(), String> {
    let held = planes.held(&plane)?;
    tauri::async_runtime::spawn_blocking(move || end_a_task(&held, session, way, below))
        .await
        .map_err(|err| format!("purlis could not end that task: {err}"))?
}

/// Stops a chat, or a chat and every chat below it. A chat another chat started gets one short
/// turn to write what it did, and the chat that asked is told the operator stopped it.
// Its plane is a `PlaneId` the registry vouches for. Not a doc comment, because the generated
// bindings carry those. **A window command and nothing else**: no hook line reaches it.
#[tauri::command]
#[specta::specta]
pub fn stop_chat(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    below: bool,
) -> Result<(), String> {
    press(&planes.held(&plane)?, session, below, Way::Report)
}

/// Every chat of a plane being stopped, so a window that has just drawn it says so.
#[tauri::command]
#[specta::specta]
pub fn stopping_chats(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<u32>, String> {
    Ok(planes.held(&plane)?.stopping().now())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chat another chat started, waiting and asking nothing.
    fn waiting() -> Facts {
        Facts {
            state: State::Waiting,
            asking: false,
            turns: 2,
            shell: false,
            dispatched: true,
            task: false,
            types: true,
        }
    }

    /// A task, on a harness purlis types into, with no key of the person's in its pane.
    fn a_task(state: State) -> Facts {
        Facts {
            state,
            task: true,
            ..waiting()
        }
    }

    fn running() -> Facts {
        Facts {
            state: State::Running,
            ..waiting()
        }
    }

    /// A chat showing a permission prompt or a question.
    fn showing_a_prompt() -> Facts {
        Facts {
            asking: true,
            ..waiting()
        }
    }

    /// A chat the person started themselves.
    fn the_person_s_own() -> Facts {
        Facts {
            dispatched: false,
            ..waiting()
        }
    }

    /// 1 started 2 and 3; 2 started 4; 4 started 5. 6 is its own, and 7 was started by 6.
    const LINEAGE: [(u32, Option<u32>); 7] = [
        (1, None),
        (2, Some(1)),
        (3, Some(1)),
        (4, Some(2)),
        (5, Some(4)),
        (6, None),
        (7, Some(6)),
    ];

    fn started(chat: u32) -> Option<u32> {
        LINEAGE
            .iter()
            .find(|(one, _)| *one == chat)
            .and_then(|(_, by)| *by)
    }

    fn ended(acts: &[Act]) -> Vec<u32> {
        acts.iter()
            .filter_map(|act| match act {
                Act::End { session, .. } => Some(*session),
                _ => None,
            })
            .collect()
    }

    // ----- which chats a stop takes -----

    #[test]
    fn stop_this_chat_takes_that_chat_and_no_other() {
        assert_eq!(subtree(&LINEAGE, 2, false), [2]);
    }

    #[test]
    fn stop_and_everything_below_takes_exactly_the_subtree_deepest_first() {
        assert_eq!(subtree(&LINEAGE, 2, true), [5, 4, 2]);
        assert_eq!(subtree(&LINEAGE, 1, true), [5, 4, 2, 3, 1]);
        assert_eq!(subtree(&LINEAGE, 5, true), [5], "nothing is below a leaf");
    }

    #[test]
    fn nothing_outside_the_subtree_is_taken() {
        let taken = subtree(&LINEAGE, 2, true);
        for outside in [1, 3, 6, 7] {
            assert!(!taken.contains(&outside), "{outside} is not below 2");
        }
    }

    #[test]
    fn a_lineage_that_loops_lists_each_chat_once() {
        let looped = [(1, Some(3)), (2, Some(1)), (3, Some(2))];
        assert_eq!(subtree(&looped, 1, true), [3, 2, 1]);
    }

    // ----- the last turn, and when its prompt is sent -----

    #[test]
    fn a_waiting_chat_another_chat_started_is_sent_its_last_turn_at_once() {
        assert_eq!(last_turn(&waiting()), LastTurn::Now);
    }

    #[test]
    fn a_chat_mid_turn_is_sent_it_when_the_turn_ends() {
        assert_eq!(last_turn(&running()), LastTurn::AtTurnEnd);
    }

    #[test]
    fn nothing_is_typed_into_a_chat_that_is_showing_a_prompt() {
        assert_eq!(last_turn(&showing_a_prompt()), LastTurn::None);
    }

    #[test]
    fn a_chat_with_nothing_to_write_or_nobody_to_write_to_gets_no_last_turn() {
        assert_eq!(last_turn(&the_person_s_own()), LastTurn::None);
        for state in [State::Unknown, State::Done, State::Failed] {
            assert_eq!(
                last_turn(&Facts { state, ..waiting() }),
                LastTurn::None,
                "{state:?}"
            );
        }
        assert_eq!(
            last_turn(&Facts {
                shell: true,
                ..waiting()
            }),
            LastTurn::None
        );
    }

    #[test]
    fn the_prompt_is_one_paste_then_enter_and_names_the_report_command() {
        let sent = sent_as(false);
        assert!(sent.starts_with("\x1b[200~The operator stopped this chat."));
        assert!(sent.ends_with("\x1b[201~\r"));
        assert!(PROMPT.contains("`purlis handoff report \"<summary>\"`"));
        // A task is told the command a task's report is taken by.
        let to_a_task = sent_as(true);
        assert!(to_a_task.starts_with("\x1b[200~The operator stopped this chat."));
        assert!(to_a_task.ends_with("\x1b[201~\r"));
        assert!(TASK_PROMPT.contains("`purlis dispatch report --outcome blocked \"<summary>\"`"));
        assert!(!TASK_PROMPT.contains("handoff report"));
    }

    #[test]
    fn the_refusals_are_fixed_words_that_name_no_chat() {
        assert_eq!(
            STARTS_NOTHING,
            "this chat is being stopped by the operator; it cannot start a chat"
        );
        assert_eq!(
            NOT_STARTED_AGAIN,
            "This chat is being stopped, so it is not started again."
        );
    }

    // ----- one chat's stop -----

    #[test]
    fn a_chat_the_person_started_is_ended_at_once() {
        let mut stops = Stops::default();

        let acts = stops.press(&[6], Way::Report, started, |_| the_person_s_own());

        assert_eq!(
            acts,
            [Act::End {
                session: 6,
                wrote: false,
                tell: true,
            }]
        );
        assert!(stops.now().is_empty());
    }

    #[test]
    fn a_chat_showing_a_prompt_is_ended_as_it_stands() {
        let mut stops = Stops::default();

        let acts = stops.press(&[2], Way::Report, started, |_| showing_a_prompt());

        assert_eq!(ended(&acts), [2]);
        assert!(
            !acts
                .iter()
                .any(|act| matches!(act, Act::Send { .. } | Act::Begun { send: true, .. })),
            "nothing is typed into it: {acts:?}"
        );
    }

    #[test]
    fn a_waiting_chat_is_sent_the_prompt_and_ends_when_that_turn_does() {
        let mut stops = Stops::default();

        let acts = stops.press(&[2], Way::Report, started, |_| waiting());
        assert_eq!(
            acts,
            [Act::Begun {
                session: 2,
                number: 1,
                send: true
            }]
        );
        assert_eq!(stops.now(), [2]);

        // A nudge, or a late report of the turn before: the last turn has not begun.
        assert!(stops.moved(2, |_| waiting()).is_empty());
        // Its last turn is under way.
        assert!(
            stops
                .moved(2, |_| Facts {
                    turns: 3,
                    ..running()
                })
                .is_empty()
        );
        // And has ended.
        let acts = stops.moved(2, |_| Facts {
            turns: 3,
            ..waiting()
        });
        assert_eq!(
            acts,
            [Act::End {
                session: 2,
                wrote: false,
                tell: true,
            }]
        );
        assert!(stops.now().is_empty());
    }

    #[test]
    fn a_chat_mid_turn_is_sent_the_prompt_when_its_turn_ends() {
        let mut stops = Stops::default();

        let acts = stops.press(&[2], Way::Report, started, |_| running());
        assert_eq!(
            acts,
            [Act::Begun {
                session: 2,
                number: 1,
                send: false
            }]
        );

        assert!(stops.moved(2, |_| running()).is_empty());
        assert_eq!(stops.moved(2, |_| waiting()), [Act::Send { session: 2 }]);
    }

    #[test]
    fn a_queued_chat_that_shows_a_prompt_is_ended_and_never_typed_into() {
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| running());

        let acts = stops.moved(2, |_| showing_a_prompt());

        assert_eq!(
            acts,
            [Act::End {
                session: 2,
                wrote: false,
                tell: true,
            }]
        );
    }

    #[test]
    fn a_last_turn_that_takes_too_long_ends_the_chat_anyway() {
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| running());

        assert_eq!(ended(&stops.timed_out(2, 1, |_| running())), [2]);
        assert!(stops.timed_out(2, 1, |_| running()).is_empty(), "once");
    }

    #[test]
    fn the_clock_of_an_earlier_stop_does_not_end_a_later_one() {
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| running());
        stops.forget(2, |_| running());
        stops.press(&[2], Way::Report, started, |_| running());

        assert!(stops.timed_out(2, 1, |_| running()).is_empty());
        assert_eq!(ended(&stops.timed_out(2, 2, |_| running())), [2]);
    }

    #[test]
    fn pressing_stop_on_a_chat_already_stopping_ends_it_now() {
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| running());

        let acts = stops.press(&[2], Way::Report, started, |_| running());

        assert_eq!(ended(&acts), [2]);
        assert!(stops.now().is_empty());
    }

    #[test]
    fn a_stopping_chat_takes_one_report_in_its_last_turn_and_no_second() {
        let mut stops = Stops::default();
        assert!(!stops.take_last_report(2), "not being stopped");
        stops.press(&[2], Way::Report, started, |_| running());
        assert!(!stops.take_last_report(2), "its prompt has not been sent");
        stops.moved(2, |_| waiting());

        assert!(stops.take_last_report(2), "the one its stop asks for");
        assert!(!stops.take_last_report(2), "tested and taken in one call");

        let acts = stops.moved(2, |_| Facts {
            turns: 3,
            ..waiting()
        });
        assert_eq!(
            acts,
            [Act::End {
                session: 2,
                wrote: true,
                tell: true,
            }]
        );
    }

    #[test]
    fn a_report_that_could_not_be_sent_is_given_back_for_one_more_try() {
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| waiting());
        assert!(stops.take_last_report(2));

        stops.give_back_last_report(2);

        assert!(stops.take_last_report(2));
        assert!(!stops.take_last_report(2));
    }

    // ----- a chat being stopped starts nothing -----

    #[test]
    fn a_chat_being_stopped_and_every_chat_below_it_starts_no_chat() {
        let mut stops = Stops::default();
        assert!(!stops.holds(2, started), "nothing is being stopped");
        // 2 alone, mid-turn: 4 and 5 are below it and not in the stop.
        stops.press(&[2], Way::Report, started, |_| running());

        assert!(stops.holds(2, started), "the chat itself");
        assert!(stops.holds(4, started), "a chat it started");
        assert!(stops.holds(5, started), "and one that chat started");
        for outside in [1, 3, 6, 7] {
            assert!(!stops.holds(outside, started), "{outside} is not under 2");
        }
        // A chat that opens under it after the press is under it too.
        let late = |chat: u32| if chat == 9 { Some(4) } else { started(chat) };
        assert!(stops.holds(9, late));
    }

    #[test]
    fn a_parent_waiting_for_its_children_starts_no_chat_either() {
        let mut stops = Stops::default();
        stops.press(&subtree(&LINEAGE, 2, true), Way::Report, started, |_| {
            waiting()
        });
        assert_eq!(stops.now(), [2, 4, 5], "2 and 4 are held while 5 writes");

        let late = |chat: u32| if chat == 9 { Some(2) } else { started(chat) };

        assert!(stops.holds(9, late), "a child the held parent would open");
    }

    #[test]
    fn a_lineage_that_loops_is_walked_to_an_end() {
        let mut stops = Stops::default();
        stops.press(&[6], Way::Report, |_| None, |_| running());
        let looped = |chat: u32| Some(if chat == 1 { 2 } else { 1 });

        assert!(!stops.holds(1, looped));
    }

    // ----- a chat and everything below it -----

    #[test]
    fn the_subtree_stop_ends_exactly_the_subtree_deepest_first() {
        let mut stops = Stops::default();
        let order = subtree(&LINEAGE, 2, true);

        // Every one of them shows a prompt, so each is ended as it stands.
        let acts = stops.press(&order, Way::Report, started, |_| showing_a_prompt());

        assert_eq!(ended(&acts), [5, 4, 2]);
        assert!(stops.now().is_empty());
    }

    #[test]
    fn a_chat_s_own_stop_waits_for_every_chat_below_it_to_end() {
        let mut stops = Stops::default();
        let order = subtree(&LINEAGE, 2, true);

        let acts = stops.press(&order, Way::Report, started, |_| waiting());
        assert_eq!(
            acts,
            [Act::Begun {
                session: 5,
                number: 1,
                send: true
            }],
            "only the deepest begins"
        );
        assert_eq!(stops.now(), [2, 4, 5]);

        // 5's last turn ends: it is ended, and 4's begins.
        let done = |_| Facts {
            turns: 3,
            ..waiting()
        };
        // 4 and 2 have had two turns; only 5 has had its third.
        let facts = |chat: u32| if chat == 5 { done(chat) } else { waiting() };
        let acts = stops.moved(5, facts);
        assert_eq!(
            acts,
            [
                Act::End {
                    session: 5,
                    wrote: false,
                    tell: false,
                },
                Act::Begun {
                    session: 4,
                    number: 2,
                    send: true
                }
            ]
        );
        // 2 does not move while 4 is still writing.
        assert!(stops.moved(2, |_| waiting()).is_empty());

        let facts = |chat: u32| if chat == 4 { done(chat) } else { waiting() };
        let acts = stops.moved(4, facts);
        assert_eq!(ended(&acts), [4]);
        assert!(matches!(
            acts[1],
            Act::Begun {
                session: 2,
                send: true,
                ..
            }
        ));
    }

    #[test]
    fn the_chat_the_person_started_at_the_top_ends_last_and_at_once() {
        let mut stops = Stops::default();
        let order = subtree(&LINEAGE, 6, true);
        let facts = |chat: u32| {
            if chat == 6 {
                the_person_s_own()
            } else {
                showing_a_prompt()
            }
        };

        assert_eq!(
            ended(&stops.press(&order, Way::Report, started, facts)),
            [7, 6]
        );
    }

    #[test]
    fn a_chat_is_not_told_of_a_stop_when_it_is_ending_in_the_same_one() {
        let mut stops = Stops::default();
        let order = subtree(&LINEAGE, 2, true);

        let acts = stops.press(&order, Way::Report, started, |_| showing_a_prompt());

        // 5's asker is 4 and 4's is 2: both are ending. 2's is 1, which is not.
        assert_eq!(
            acts,
            [
                Act::End {
                    session: 5,
                    wrote: false,
                    tell: false,
                },
                Act::End {
                    session: 4,
                    wrote: false,
                    tell: false,
                },
                Act::End {
                    session: 2,
                    wrote: false,
                    tell: true,
                },
            ]
        );
    }

    #[test]
    fn everything_below_pressed_on_a_chat_stopping_alone_gives_the_others_an_ordinary_stop() {
        let mut stops = Stops::default();
        // 2 was stopped alone while mid-turn: its prompt waits for the turn to end.
        stops.press(&[2], Way::Report, started, |_| running());

        let acts = stops.press(&subtree(&LINEAGE, 2, true), Way::Report, started, |_| {
            running()
        });

        assert!(ended(&acts).is_empty(), "nothing is ended early: {acts:?}");
        assert_eq!(
            acts,
            [Act::Begun {
                session: 5,
                number: 2,
                send: false
            }],
            "the deepest begins its own stop, last turn and all"
        );
        assert_eq!(stops.now(), [2, 4, 5]);
        // 2 now waits for the chats below it: its turn ending sends it nothing yet.
        assert!(stops.moved(2, |_| waiting()).is_empty());
        // And the clock of its first stop does not end it while it waits.
        assert!(stops.timed_out(2, 1, |_| running()).is_empty());
    }

    #[test]
    fn a_chat_already_writing_its_last_turn_carries_on_when_the_rest_are_added() {
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| waiting());

        let acts = stops.press(&subtree(&LINEAGE, 2, true), Way::Report, started, |_| {
            waiting()
        });

        assert!(ended(&acts).is_empty());
        assert_eq!(stops.now(), [2, 4, 5]);
        // 2's last turn ends: it ends, told to its asker, while the others write theirs.
        let facts = |chat: u32| {
            if chat == 2 {
                Facts {
                    turns: 3,
                    ..waiting()
                }
            } else {
                waiting()
            }
        };
        assert_eq!(ended(&stops.moved(2, facts)), [2]);
    }

    #[test]
    fn a_second_press_on_the_subtree_ends_all_of_it_now_deepest_first() {
        let mut stops = Stops::default();
        let order = subtree(&LINEAGE, 2, true);
        stops.press(&order, Way::Report, started, |_| waiting());

        let acts = stops.press(&order, Way::Report, started, |_| waiting());

        assert_eq!(ended(&acts), [5, 4, 2]);
        assert!(stops.now().is_empty());
    }

    #[test]
    fn a_chat_closed_by_its_tab_frees_the_chat_above_it() {
        let mut stops = Stops::default();
        stops.press(&subtree(&LINEAGE, 4, true), Way::Report, started, |_| {
            waiting()
        });

        let acts = stops.forget(5, |_| waiting());

        assert_eq!(
            acts,
            [Act::Begun {
                session: 4,
                number: 2,
                send: true
            }],
            "5 is not ended twice, and 4 begins"
        );
    }

    #[test]
    fn a_program_that_ends_on_its_own_while_stopping_is_ended_as_a_stop_ends_it() {
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| running());

        assert_eq!(ended(&stops.give_up(2, |_| running())), [2]);
        assert!(stops.give_up(2, |_| running()).is_empty(), "not stopping");
    }

    // ----- the person ends a task: Stop and get its report, or Close now (#1488) -----

    #[test]
    fn a_task_mid_turn_has_its_turn_ended_then_is_asked_and_ends_when_that_turn_does() {
        let mut stops = Stops::default();
        let acts = stops.press(&[2], Way::Report, started, |_| a_task(State::Running));
        // Begun, with nothing sent yet, and its turn ended as the person's Escape would.
        let number = stops.number(2).expect("stopping");
        assert_eq!(
            acts,
            [
                Act::Begun {
                    session: 2,
                    number,
                    send: false
                },
                Act::Interrupt { session: 2, number },
            ]
        );
        // No harness reports an interrupted turn: the board still says running, and nothing
        // is sent until the turn has had its moment to stop.
        assert!(stops.moved(2, |_| a_task(State::Running)).is_empty());
        assert_eq!(
            stops.settled(2, number, |_| a_task(State::Running)),
            [Act::Send { session: 2 }]
        );
        // A second look after the line is sent sends no second line.
        assert!(
            stops
                .settled(2, number, |_| a_task(State::Running))
                .is_empty()
        );
        // The line began a turn (one more than it had had), and that turn has ended.
        let after = |_| Facts {
            turns: 3,
            ..a_task(State::Waiting)
        };
        assert!(stops.take_last_report(2));
        assert_eq!(
            stops.moved(2, after),
            [Act::End {
                session: 2,
                wrote: true,
                tell: true
            }]
        );
        assert!(!stops.any());
    }

    #[test]
    fn a_waiting_task_is_asked_at_once_and_one_purlis_may_not_type_into_is_ended_as_it_stands() {
        let mut stops = Stops::default();
        let acts = stops.press(&[2], Way::Report, started, |_| a_task(State::Waiting));
        assert!(
            matches!(acts[..], [Act::Begun { send: true, .. }]),
            "{acts:?}"
        );

        // Each place purlis types nothing: a prompt, the person's keys or a harness it does
        // not type into, and one it has heard nothing from.
        let prompt = Facts {
            asking: true,
            ..a_task(State::Waiting)
        };
        let not_typed_into = |state| Facts {
            types: false,
            ..a_task(state)
        };
        for facts in [
            prompt,
            not_typed_into(State::Waiting),
            not_typed_into(State::Running),
            a_task(State::Unknown),
        ] {
            let mut stops = Stops::default();
            let acts = stops.press(&[2], Way::Report, started, |_| facts);
            assert_eq!(
                acts,
                [Act::End {
                    session: 2,
                    wrote: false,
                    tell: true
                }],
                "{facts:?}"
            );
            assert!(!stops.any(), "never left stopping: {facts:?}");
        }
    }

    #[test]
    fn a_task_that_shows_a_prompt_or_is_typed_in_before_its_line_is_sent_is_never_typed_into() {
        for now in [
            Facts {
                asking: true,
                ..a_task(State::Waiting)
            },
            Facts {
                types: false,
                ..a_task(State::Running)
            },
            a_task(State::Done),
        ] {
            let mut stops = Stops::default();
            stops.press(&[2], Way::Report, started, |_| a_task(State::Running));
            let number = stops.number(2).unwrap();
            let acts = stops.settled(2, number, |_| now);
            assert_eq!(ended(&acts), [2], "{now:?}");
            assert!(
                !acts.iter().any(|act| matches!(act, Act::Send { .. })),
                "{now:?}"
            );
        }
        // And one that comes to show a prompt while its turn settles is ended as it moves.
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| a_task(State::Running));
        let prompt = |_| Facts {
            asking: true,
            ..a_task(State::Waiting)
        };
        assert_eq!(ended(&stops.moved(2, prompt)), [2]);
    }

    #[test]
    fn a_line_that_starts_no_turn_ends_the_stop_and_one_that_did_is_left_to_its_turn() {
        // The bounded wait: the line was swallowed, and no turn began on it.
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| a_task(State::Waiting));
        let number = stops.number(2).unwrap();
        assert_eq!(
            stops.unheard(2, number, |_| a_task(State::Waiting)),
            [Act::End {
                session: 2,
                wrote: false,
                tell: true
            }]
        );
        assert!(!stops.any());

        // A turn did begin: this clock ends nothing, and the last turn's own clock still does.
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| a_task(State::Waiting));
        let number = stops.number(2).unwrap();
        let began = |_| Facts {
            turns: 3,
            ..a_task(State::Running)
        };
        assert!(stops.unheard(2, number, began).is_empty());
        assert!(stops.has(2));
        assert_eq!(ended(&stops.timed_out(2, number, began)), [2]);

        // The clock of an earlier stop of the chat ends no later one.
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| a_task(State::Running));
        let earlier = stops.number(2).unwrap();
        stops.give_up(2, |_| a_task(State::Running));
        stops.press(&[2], Way::Report, started, |_| a_task(State::Running));
        assert!(
            stops
                .settled(2, earlier, |_| a_task(State::Running))
                .is_empty()
        );
        assert!(
            stops
                .unheard(2, earlier, |_| a_task(State::Running))
                .is_empty()
        );
        assert!(stops.has(2));
    }

    #[test]
    fn a_report_a_task_sends_once_its_stop_is_recorded_is_the_stop_s_at_any_step() {
        // Sent before it was asked, in the turn that was being ended: the stop's report all
        // the same, and it is then sent no line.
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| a_task(State::Running));
        let number = stops.number(2).unwrap();
        assert!(stops.take_last_report(2));
        assert!(!stops.take_last_report(2), "one report, and no second");
        assert!(
            stops
                .settled(2, number, |_| a_task(State::Running))
                .is_empty()
        );
        assert_eq!(
            stops.moved(2, |_| a_task(State::Waiting)),
            [Act::End {
                session: 2,
                wrote: true,
                tell: true
            }]
        );

        // Sent while it waited for the tasks below it: ended when they have, and not asked.
        let mut stops = Stops::default();
        let order = subtree(&LINEAGE, 4, true);
        stops.press(&order, Way::Report, started, |_| a_task(State::Waiting));
        assert!(stops.take_last_report(4), "held, and a task");
        let after = |_| Facts {
            turns: 3,
            ..a_task(State::Waiting)
        };
        let acts = stops.moved(5, after);
        assert_eq!(ended(&acts), [5, 4]);
        assert!(
            !acts
                .iter()
                .any(|act| matches!(act, Act::Begun { session: 4, .. })),
            "{acts:?}"
        );

        // A chat handed its work is as it was: its one report is its last turn's.
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| running());
        assert!(!stops.take_last_report(2));
    }

    #[test]
    fn close_now_ends_a_task_at_once_whatever_it_is_doing_and_everything_below_deepest_first() {
        for facts in [
            a_task(State::Running),
            a_task(State::Waiting),
            a_task(State::Unknown),
            Facts {
                asking: true,
                ..a_task(State::Waiting)
            },
        ] {
            let mut stops = Stops::default();
            let acts = stops.press(&[2], Way::Now, started, |_| facts);
            assert_eq!(
                acts,
                [Act::End {
                    session: 2,
                    wrote: false,
                    tell: true
                }],
                "no turn, no line, no wait: {facts:?}"
            );
            assert!(!stops.any());
        }
        // With what is below it: deepest first, and only the chat it was pressed on is told
        // of, since each chat that asked for the others is ending in the same stop.
        let mut stops = Stops::default();
        let acts = stops.press(&subtree(&LINEAGE, 2, true), Way::Now, started, |_| {
            a_task(State::Running)
        });
        assert_eq!(
            acts,
            [
                Act::End {
                    session: 5,
                    wrote: false,
                    tell: false
                },
                Act::End {
                    session: 4,
                    wrote: false,
                    tell: false
                },
                Act::End {
                    session: 2,
                    wrote: false,
                    tell: true
                },
            ]
        );
        // On a task already being stopped, it ends it without waiting for its turn, and a
        // report it had sent is still the stop's.
        let mut stops = Stops::default();
        stops.press(&[2], Way::Report, started, |_| a_task(State::Waiting));
        assert!(stops.take_last_report(2));
        assert_eq!(
            stops.press(&[2], Way::Now, started, |_| a_task(State::Running)),
            [Act::End {
                session: 2,
                wrote: true,
                tell: true
            }]
        );
    }

    // ----- only the person -----
    //
    // `press` is private to this file, so the compiler holds that `stop_chat` and `end_task`
    // are its only callers. What every ask on the socket does to a plane's stops is tested where the asks are
    // answered (`handoff.rs`).

    #[test]
    fn no_line_on_the_hook_socket_reads_as_a_stop() {
        // A chat's asks are the wire's `Ask`, and no spelling of a stop reads as one: the wire
        // has no such ask to read it as. (Whatever asks it gains, none reaches `press`: the
        // test above holds that.)
        use purlis_core::hookwire::Ask;
        for forged in [
            r#"{"stop":{"chat":2}}"#,
            r#"{"stop_chat":{"chat":2,"below":true}}"#,
            r#"{"stop":{"chat":2,"session":3,"below":true}}"#,
            r#"{"stop_chat":{"plane":"/plane","session":3,"below":false}}"#,
            r#""stop""#,
            r#""stop_chat""#,
            // Nor of the two ways the person ends a task (#1488).
            r#"{"end_task":{"chat":2,"way":"now","below":false}}"#,
            r#"{"end_task":{"session":2,"way":"report","below":true}}"#,
            r#"{"task":{"chat":3,"what":{"end":{"of":2,"way":"now"}}}}"#,
            r#"{"task":{"chat":3,"what":{"close_now":{"of":2}}}}"#,
            r#"{"task":{"chat":3,"what":{"stop":{"of":2}}}}"#,
        ] {
            assert!(
                serde_json::from_str::<Ask>(forged).is_err(),
                "{forged} read as an ask"
            );
        }
        // The one ask that names a chat alone still reads, so the check above is of the words.
        assert!(serde_json::from_str::<Ask>(r#"{"ticket":{"chat":2}}"#).is_ok());
    }
}
