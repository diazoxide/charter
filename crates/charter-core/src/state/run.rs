//! A run's state, and the only way it moves (ADR 0076).
//!
//! A run is in one of nine states, and every move names exactly one [`Cause`]: a fact from a
//! hook, the harness's protocol, the program's exit, or an act of the host, the operator or a
//! policy. Nothing here reads a harness's output. The three end states are final.
//!
//! # A move, no move, or a refusal
//!
//! [`Run::apply`] answers one of three things, by one rule:
//!
//! - **a [`Move`]**, when ADR 0076's table has a row for this cause from this state;
//! - **`Ok(None)`, for an expected fact that moves nothing**: one the ADR says leaves the run
//!   where it is (a host crash on a run with no process, a quit on a hibernated run, the exit
//!   a stop or a hibernation left behind), or one a harness sends routinely in that state
//!   without it meaning a move (a tool hook mid-turn, an ask repeated, an idle nudge, the spawn
//!   of a harness that reports its own start);
//! - **a [`Refusal`], for an impossible one**: anything else the table has no row for, such
//!   as admitting a run that still has a hold, pausing a paused run, a turn ending before any
//!   turn began, or any cause at a run that has ended. The run does not move.
//!
//! This differs from [`super::Chat`]'s `bool` ("did anything a reader can see change") on
//! purpose. A run's move is what the event log and the audit record, so the caller needs the
//! move itself, with its cause, and needs a refusal to be something it can log and count
//! rather than a quiet `false`: a refused fact is either a harness doing something new or a
//! caller breaking the table, and both want finding. The chat board still answers `bool`
//! until it moves onto runs (#791).

use std::collections::BTreeSet;

/// Why a queued run is not admitted yet (ADR 0076 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hold {
    /// The operator has not answered the launch question.
    Launch,
    /// Its turn in a relaunch's staggered restart has not come (SC-20).
    Stagger,
    /// The chat's budget is spent (N4).
    Budget,
    /// A trigger queue or a cap on running chats is full (AC-7).
    Capacity,
    /// Free disk is below the admission guard (V8).
    Disk,
}

/// Why an `input-required` run handed control back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// A question or a permission, in the middle of a turn.
    Asked,
    /// The turn is over.
    TurnEnded,
    /// A session began, and nothing has been asked.
    Ready,
}

/// A run's state (ADR 0076 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Queued,
    Starting,
    Working,
    InputRequired(Reason),
    Paused,
    Hibernated,
    Completed,
    Failed,
    Stopped,
}

impl RunState {
    /// Whether this is one of the three end states, which a run never leaves.
    pub fn is_end(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Stopped)
    }

    /// Whether a run in this state has a program running: what can be signalled, can exit,
    /// and can carry on into a successor run.
    pub fn has_a_process(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Working | Self::InputRequired(_) | Self::Paused
        )
    }
}

/// The level a run was started at (ADR 0073).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    One,
    Two,
    Three,
}

/// What a run's harness declaration says about it, fixed for the run (ADR 0066, ADR 0073).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub level: Level,
    /// The declaration's `reports_its_start_before_the_first_prompt`.
    pub reports_start: bool,
    /// The declaration's `resumes_by_id`: whether the harness resumes natively.
    pub resumes_natively: bool,
}

/// The gate that refused a start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    KillSwitch,
    MinimumLevel,
    Declaration,
}

/// Who paused a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseBy {
    /// The chat's budget ran out (N4).
    Budget,
    /// Anomaly detection or a policy (N8).
    Policy,
    /// The operator, from a human client scope.
    Operator,
}

/// Why the host, the operator or a policy ended a run on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBy {
    /// The operator closed the chat.
    Closed,
    /// The operator stopped this run and kept its tab.
    Operator,
    /// The kill switch.
    Killed,
    /// The app let go of the project.
    Quit,
    /// The app crashed and did not come back within the grace period.
    Grace,
    /// An upgrade fell back to drain and resume.
    Drained,
}

/// How a run's program ended, as the host learned it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// It exited with this code.
    Code(i32),
    /// It ended on a signal, and left no code.
    Signal,
    /// It was handed over in an upgrade and its code is lost; whether a `SessionEnd` for good
    /// came first (V27b).
    Lost { session_ended: bool },
}

/// Why a move happened (ADR 0076 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Admitted,
    Refused(Gate),
    SessionReported,
    Spawned,
    Prompted,
    Answered,
    Asked,
    TurnEnded,
    Pause(PauseBy),
    Unpaused,
    Idle,
    Superseded,
    Exited(Exit),
    SpawnFailed,
    ChannelLost,
    HostCrash,
    Stop(StopBy),
    ChildEnded,
    Observed(RunState),
}

impl Cause {
    /// The cause's word, as ADR 0076 writes it and an event records it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Refused(_) => "refused",
            Self::SessionReported => "session-reported",
            Self::Spawned => "spawned",
            Self::Prompted => "prompted",
            Self::Answered => "answered",
            Self::Asked => "asked",
            Self::TurnEnded => "turn-ended",
            Self::Pause(PauseBy::Budget) => "budget",
            Self::Pause(PauseBy::Policy) => "policy",
            Self::Pause(PauseBy::Operator) | Self::Stop(StopBy::Operator) => "operator",
            Self::Unpaused => "unpaused",
            Self::Idle => "idle",
            Self::Superseded => "superseded",
            Self::Exited(_) => "exited",
            Self::SpawnFailed => "spawn-failed",
            Self::ChannelLost => "channel-lost",
            Self::HostCrash => "host-crash",
            Self::Stop(StopBy::Closed) => "closed",
            Self::Stop(StopBy::Killed) => "killed",
            Self::Stop(StopBy::Quit) => "quit",
            Self::Stop(StopBy::Grace) => "grace",
            Self::Stop(StopBy::Drained) => "drained",
            Self::ChildEnded => "child-ended",
            Self::Observed(_) => "observed",
        }
    }
}

/// One move of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: RunState,
    pub to: RunState,
    pub cause: Cause,
}

/// A cause that may not move this run from where it is. The run did not move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    pub state: RunState,
    pub cause: Cause,
}

/// Which kind of run this is: one charter governs, a child agent's, or a remote chat's
/// (ADR 0076 §6, §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Governed,
    Child,
    Observed,
}

/// Why a child run could not end with its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentRefusal {
    /// This run is not a child run.
    NotAChild,
    /// The parent has not ended; it is in this state.
    ParentLive(RunState),
    /// The child has already ended, in this state.
    AlreadyEnded(RunState),
}

/// One run: its state, and everything that may move it.
#[derive(Debug, Clone)]
pub struct Run {
    kind: Kind,
    facts: Facts,
    state: RunState,
    holds: BTreeSet<Hold>,
    /// The state a paused run goes back to, and who paused it.
    paused: Option<(RunState, PauseBy)>,
    /// Why the host is about to end the run's program, written before it signals.
    stopping: Option<StopBy>,
    /// Whether the run left its program behind when it moved: a stop or a hibernation the host
    /// made before the program's exit reached it. That exit is still the run's to record.
    exit_owed: bool,
    /// How the program ended, once it has.
    exit: Option<Exit>,
    /// The cause of the move that ended the run, once it has ended.
    ended_by: Option<Cause>,
    /// Whether the operator has been shown the chat since the run ended.
    seen: bool,
}

/// What a move out of `from` for `cause` does: the state it goes to, or [`Step::Stay`] for an
/// expected fact that moves nothing. `None` from a step function is a refusal.
enum Step {
    To(RunState),
    Stay,
}

impl Run {
    /// A run the host was asked to start: `(none) → queued | requested`.
    pub fn requested(facts: Facts, holds: &[Hold]) -> Self {
        Self {
            kind: Kind::Governed,
            facts,
            state: RunState::Queued,
            holds: holds.iter().copied().collect(),
            paused: None,
            stopping: None,
            exit_owed: false,
            exit: None,
            ended_by: None,
            seen: false,
        }
    }

    /// A child run: a sub-agent the harness spawned, seen for the first time
    /// (`(none) → working | child-seen`).
    pub fn child(facts: Facts) -> Self {
        Self {
            kind: Kind::Child,
            state: RunState::Working,
            ..Run::requested(facts, &[])
        }
    }

    /// A run of a remote chat, which charter only observes (`observed` kind), as its vendor
    /// first reports it. Refused in a state a vendor cannot report (ADR 0076 §7).
    pub fn observed(state: RunState, facts: Facts) -> Result<Self, Refusal> {
        if !Self::observable(state) {
            return Err(Refusal {
                state,
                cause: Cause::Observed(state),
            });
        }
        Ok(Self {
            kind: Kind::Observed,
            state,
            ..Run::requested(facts, &[])
        })
    }

    /// A new run beginning in the same process as `previous` (`clear`, or a `switch` that keeps
    /// the process): `(none) → <previous's state> | succeeded`. `previous` ends `completed |
    /// superseded` in the same step, and that move is answered beside the new run.
    ///
    /// Refused, with `previous` untouched, unless `previous` has a live process: a queued or
    /// hibernated run has none to carry on in, and an ended one is over.
    pub fn succeeding(previous: &mut Run, facts: Facts) -> Result<(Run, Move), Refusal> {
        if previous.kind != Kind::Governed || !previous.state.has_a_process() {
            return Err(Refusal {
                state: previous.state,
                cause: Cause::Superseded,
            });
        }
        let next = Run {
            state: previous.state,
            paused: previous.paused,
            ..Run::requested(facts, &[])
        };
        let ended = previous.moved(previous.state, RunState::Completed, Cause::Superseded);
        Ok((next, ended))
    }

    /// A child run's parent has ended: the child ends with it, in the parent's end state and
    /// for the parent's cause.
    pub fn end_with_parent(&mut self, parent: &Run) -> Result<Move, ParentRefusal> {
        let from = self.state;
        if self.kind != Kind::Child {
            return Err(ParentRefusal::NotAChild);
        }
        if from.is_end() {
            return Err(ParentRefusal::AlreadyEnded(from));
        }
        match parent.ended_by {
            Some(cause) => Ok(self.moved(from, parent.state, cause)),
            None => Err(ParentRefusal::ParentLive(parent.state)),
        }
    }

    /// The host is about to signal this run's program to end it, for `by`. Not a move: the
    /// exit that follows is, through the same table as [`Cause::Stop`]. Refused where that
    /// table refuses the stop, and where there is no program to signal.
    pub fn stopping(&mut self, by: StopBy) -> Result<(), Refusal> {
        let refusal = Refusal {
            state: self.state,
            cause: Cause::Stop(by),
        };
        if self.kind != Kind::Governed || !self.state.has_a_process() {
            return Err(refusal);
        }
        match self.stop_step(by) {
            Some(Step::To(_)) => {
                self.stopping = Some(by);
                Ok(())
            }
            Some(Step::Stay) | None => Err(refusal),
        }
    }

    /// Whether this run puts its chat in needs you (ADR 0076 §9).
    pub fn needs_you(&self) -> bool {
        match self.state {
            RunState::InputRequired(reason) => reason != Reason::Ready,
            RunState::Paused => matches!(self.paused, Some((_, PauseBy::Budget | PauseBy::Policy))),
            RunState::Queued => self.holds.contains(&Hold::Budget),
            // Settled by W10: the queue shows finished and failed chats. A run superseded by
            // the next is not finished: its chat carries on.
            RunState::Completed => !self.seen && matches!(self.ended_by, Some(Cause::Exited(_))),
            RunState::Failed => !self.seen,
            RunState::Starting | RunState::Working | RunState::Hibernated | RunState::Stopped => {
                false
            }
        }
    }

    /// The operator has been shown the chat. An end it was waiting to see is seen.
    pub fn shown(&mut self) {
        if self.state.is_end() {
            self.seen = true;
        }
    }

    /// Where the run is.
    pub fn state(&self) -> RunState {
        self.state
    }

    /// What a queued run still waits for.
    pub fn holds(&self) -> impl Iterator<Item = Hold> + '_ {
        self.holds.iter().copied()
    }

    /// One thing a queued run waited for is no longer in the way. Not a move.
    pub fn release(&mut self, hold: Hold) {
        self.holds.remove(&hold);
    }

    /// How the run's program ended, once it has: what `run.ended` records as its exit code
    /// (ADR 0075, amended).
    pub fn exit(&self) -> Option<Exit> {
        self.exit
    }

    /// `cause` happened. Answers the move it made, `None` for an expected fact that moves
    /// nothing, or a [`Refusal`] for a fact that cannot come from here (see the module docs).
    pub fn apply(&mut self, cause: Cause) -> Result<Option<Move>, Refusal> {
        let from = self.state;
        let refusal = Refusal { state: from, cause };
        if let Cause::Exited(exit) = cause
            && self.exit_owed
        {
            // The exit a stop or a hibernation left behind: recorded, not a move.
            self.exit_owed = false;
            self.exit = Some(exit);
            return Ok(None);
        }
        if from.is_end() {
            return Err(refusal);
        }
        let (step, cause) = match (self.kind, cause) {
            (Kind::Governed, Cause::Exited(exit)) => self.read_exit_against_the_stop_intent(exit),
            (Kind::Governed, _) => (self.governed_step(cause), cause),
            (Kind::Child, Cause::ChildEnded) => (Some(Step::To(RunState::Completed)), cause),
            (Kind::Observed, Cause::Observed(to)) if to != from && Self::observable(to) => {
                (Some(Step::To(to)), cause)
            }
            (Kind::Child | Kind::Observed, _) => (None, cause),
        };
        match step {
            None => Err(refusal),
            Some(Step::Stay) => Ok(None),
            Some(Step::To(to)) => {
                if from.has_a_process()
                    && matches!(cause, Cause::Stop(_) | Cause::Idle)
                    && self.exit.is_none()
                {
                    self.exit_owed = true;
                }
                match cause {
                    Cause::Pause(by) => self.paused = Some((from, by)),
                    Cause::Unpaused => self.paused = None,
                    _ => {}
                }
                Ok(Some(self.moved(from, to, cause)))
            }
        }
    }

    /// The program exited. After the host said it would stop the run, the exit is that stop,
    /// read through the stop's own table (a quit hibernates an idle run), whatever the code
    /// says. Otherwise the code decides: 0, or a lost code after a `SessionEnd` (V27b),
    /// completes the run, and anything else fails it. The exit is recorded either way.
    fn read_exit_against_the_stop_intent(&mut self, exit: Exit) -> (Option<Step>, Cause) {
        if !self.state.has_a_process() {
            // Nothing was running to exit: the exit cannot come from here.
            return (None, Cause::Exited(exit));
        }
        self.exit = Some(exit);
        if let Some(by) = self.stopping.take() {
            // The host signalled. A stop is a stop, even if the run moved since the intent.
            let to = match self.stop_step(by) {
                Some(Step::To(to)) => to,
                Some(Step::Stay) | None => RunState::Stopped,
            };
            return (Some(Step::To(to)), Cause::Stop(by));
        }
        let to = match exit {
            Exit::Code(0)
            | Exit::Lost {
                session_ended: true,
            } => RunState::Completed,
            Exit::Code(_) | Exit::Signal | Exit::Lost { .. } => RunState::Failed,
        };
        (Some(Step::To(to)), Cause::Exited(exit))
    }

    /// A governed run's move for every cause but the exit.
    fn governed_step(&mut self, cause: Cause) -> Option<Step> {
        match cause {
            Cause::Admitted | Cause::Refused(_) | Cause::SpawnFailed => self.admission_step(cause),
            Cause::SessionReported
            | Cause::Spawned
            | Cause::Prompted
            | Cause::Answered
            | Cause::Asked
            | Cause::TurnEnded => self.turn_step(cause),
            Cause::Pause(_) | Cause::Unpaused | Cause::Idle => self.hold_step(cause),
            Cause::Stop(by) => self.stop_step(by),
            Cause::Superseded => Some(Step::To(RunState::Completed)),
            Cause::ChannelLost | Cause::HostCrash => self.failure_step(cause),
            Cause::Exited(_) | Cause::ChildEnded | Cause::Observed(_) => None,
        }
    }

    /// `admitted`, `refused` and `spawn-failed`: the start and its failures.
    fn admission_step(&self, cause: Cause) -> Option<Step> {
        let to = match (self.state, cause) {
            (RunState::Queued, Cause::Admitted) if self.holds.is_empty() => RunState::Starting,
            (RunState::Queued, Cause::Refused(_)) => RunState::Stopped,
            (RunState::Queued | RunState::Starting, Cause::SpawnFailed) => RunState::Failed,
            _ => return None,
        };
        Some(Step::To(to))
    }

    /// The facts a harness reports about its session and its turns.
    fn turn_step(&self, cause: Cause) -> Option<Step> {
        let to = match (self.state, cause) {
            (RunState::Starting, Cause::SessionReported) if self.facts.level != Level::One => {
                RunState::InputRequired(Reason::Ready)
            }
            (RunState::Starting, Cause::Spawned) => match self.facts {
                Facts {
                    level: Level::One, ..
                } => RunState::Working,
                Facts {
                    level: Level::Two,
                    reports_start: false,
                    ..
                } => RunState::InputRequired(Reason::Ready),
                // It will report its own start: the spawn is expected and moves nothing.
                _ => return Some(Step::Stay),
            },
            (RunState::Starting | RunState::InputRequired(_), Cause::Prompted) => RunState::Working,
            (RunState::InputRequired(Reason::Asked), Cause::Answered) => RunState::Working,
            // A tool hook in a turn that is going on anyway.
            (RunState::Working, Cause::Answered) => return Some(Step::Stay),
            (RunState::Working, Cause::Asked) => RunState::InputRequired(Reason::Asked),
            // Claude Code's idle nudge, or an ask repeated: routine, and nothing new.
            (RunState::InputRequired(_), Cause::Asked) => return Some(Step::Stay),
            (RunState::Working | RunState::InputRequired(Reason::Asked), Cause::TurnEnded) => {
                RunState::InputRequired(Reason::TurnEnded)
            }
            _ => return None,
        };
        Some(Step::To(to))
    }

    /// `budget`, `policy`, `operator` pauses, `unpaused` and `idle`: the holds in place.
    fn hold_step(&self, cause: Cause) -> Option<Step> {
        let to = match (self.state, cause) {
            (RunState::Working | RunState::InputRequired(_), Cause::Pause(_)) => RunState::Paused,
            (RunState::Paused, Cause::Unpaused) => self.paused.map(|(back, _)| back)?,
            (RunState::InputRequired(_), Cause::Idle) if self.can_hibernate() => {
                RunState::Hibernated
            }
            _ => return None,
        };
        Some(Step::To(to))
    }

    /// A stop the operator, a policy or the host chose, from wherever the run is: the one table
    /// both [`Cause::Stop`] and an exit after [`Run::stopping`] read.
    fn stop_step(&self, by: StopBy) -> Option<Step> {
        let to = match (self.state, by) {
            (RunState::Hibernated, StopBy::Quit | StopBy::Grace) => return Some(Step::Stay),
            (_, StopBy::Quit | StopBy::Grace) if self.can_hibernate() => RunState::Hibernated,
            (RunState::InputRequired(_), StopBy::Drained) => RunState::Stopped,
            (_, StopBy::Drained) => return None,
            (state, _) if state.is_end() => return None,
            _ => RunState::Stopped,
        };
        Some(Step::To(to))
    }

    /// `channel-lost` and `host-crash`: ends nobody chose, besides the exit.
    fn failure_step(&self, cause: Cause) -> Option<Step> {
        let to = match (self.state, cause) {
            (RunState::Working | RunState::InputRequired(_), Cause::ChannelLost)
                if self.facts.level == Level::Three =>
            {
                RunState::Failed
            }
            // No process to lose: the crash leaves it as it is.
            (RunState::Queued | RunState::Hibernated, Cause::HostCrash) => return Some(Step::Stay),
            (RunState::InputRequired(_), Cause::HostCrash) if self.can_hibernate() => {
                RunState::Hibernated
            }
            (
                RunState::Starting
                | RunState::Working
                | RunState::Paused
                | RunState::InputRequired(_),
                Cause::HostCrash,
            ) => RunState::Failed,
            _ => return None,
        };
        Some(Step::To(to))
    }

    /// Whether the run is idle at a turn boundary on a harness that resumes natively: what may
    /// be hibernated without losing anything (ADR 0076 §5, V27a).
    fn can_hibernate(&self) -> bool {
        self.facts.resumes_natively
            && matches!(
                self.state,
                RunState::InputRequired(Reason::Ready | Reason::TurnEnded)
            )
    }

    /// The states a vendor can report for a remote chat's run (ADR 0076 §7): never one of
    /// charter's own holds.
    fn observable(state: RunState) -> bool {
        !matches!(
            state,
            RunState::Queued | RunState::Starting | RunState::Paused | RunState::Hibernated
        )
    }

    /// Makes the move, and remembers the cause that ended the run if it did.
    fn moved(&mut self, from: RunState, to: RunState, cause: Cause) -> Move {
        self.state = to;
        if to.is_end() {
            self.ended_by = Some(cause);
        }
        Move { from, to, cause }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO: Facts = Facts {
        level: Level::Two,
        reports_start: true,
        resumes_natively: true,
    };

    fn moved(from: RunState, to: RunState, cause: Cause) -> Result<Option<Move>, Refusal> {
        Ok(Some(Move { from, to, cause }))
    }

    /// A run of `facts` that is already `starting`.
    fn starting(facts: Facts) -> Run {
        let mut run = Run::requested(facts, &[]);
        run.apply(Cause::Admitted).unwrap();
        run
    }

    /// A level-2 run in `state`, reached through the causes that lead there.
    fn at(state: RunState) -> Run {
        let mut run = starting(TWO);
        let path: &[Cause] = match state {
            RunState::Starting => &[],
            RunState::InputRequired(Reason::Ready) => &[Cause::SessionReported],
            RunState::Working => &[Cause::SessionReported, Cause::Prompted],
            RunState::InputRequired(Reason::Asked) => {
                &[Cause::SessionReported, Cause::Prompted, Cause::Asked]
            }
            RunState::InputRequired(Reason::TurnEnded) => {
                &[Cause::SessionReported, Cause::Prompted, Cause::TurnEnded]
            }
            other => panic!("no path to {other:?} here"),
        };
        for cause in path {
            run.apply(*cause).unwrap();
        }
        assert_eq!(run.state(), state);
        run
    }

    // ----- queued: requested, admitted, refused ---------------------------------------------

    #[test]
    fn a_requested_run_is_admitted_once_its_last_hold_is_released() {
        let mut run = Run::requested(TWO, &[Hold::Launch, Hold::Stagger]);
        assert_eq!(run.state(), RunState::Queued);
        assert!(run.apply(Cause::Admitted).is_err());
        run.release(Hold::Launch);
        assert!(run.apply(Cause::Admitted).is_err());
        run.release(Hold::Stagger);
        assert_eq!(
            run.apply(Cause::Admitted),
            moved(RunState::Queued, RunState::Starting, Cause::Admitted)
        );
    }

    #[test]
    fn a_gate_refusing_the_start_stops_the_queued_run() {
        let mut run = Run::requested(TWO, &[Hold::Launch]);
        assert_eq!(
            run.apply(Cause::Refused(Gate::KillSwitch)),
            moved(
                RunState::Queued,
                RunState::Stopped,
                Cause::Refused(Gate::KillSwitch)
            )
        );
        assert!(run.apply(Cause::Admitted).is_err(), "an end is final");
    }

    // ----- starting: session-reported, spawned ----------------------------------------------

    #[test]
    fn a_session_start_report_makes_a_starting_run_ready() {
        let mut run = starting(TWO);
        assert_eq!(
            run.apply(Cause::SessionReported),
            moved(
                RunState::Starting,
                RunState::InputRequired(Reason::Ready),
                Cause::SessionReported
            )
        );
    }

    #[test]
    fn a_level_one_run_is_working_from_the_moment_its_program_starts() {
        let mut run = starting(Facts {
            level: Level::One,
            ..TWO
        });
        assert_eq!(
            run.apply(Cause::Spawned),
            moved(RunState::Starting, RunState::Working, Cause::Spawned)
        );
        assert!(
            run.apply(Cause::SessionReported).is_err(),
            "a level-1 run has no hooks to report a session"
        );
    }

    #[test]
    fn a_level_two_harness_that_reports_nothing_before_a_prompt_is_ready_once_spawned() {
        let mut run = starting(Facts {
            reports_start: false,
            ..TWO
        });
        assert_eq!(
            run.apply(Cause::Spawned),
            moved(
                RunState::Starting,
                RunState::InputRequired(Reason::Ready),
                Cause::Spawned
            )
        );
    }

    #[test]
    fn a_harness_that_reports_its_own_start_waits_for_the_report_not_the_spawn() {
        let mut run = starting(TWO);
        assert_eq!(run.apply(Cause::Spawned), Ok(None));
        assert_eq!(run.state(), RunState::Starting);
    }

    // ----- the turn: prompted, asked, answered, turn-ended ----------------------------------

    #[test]
    fn a_prompt_starts_a_turn_from_starting_or_from_any_input_required() {
        for state in [
            RunState::Starting,
            RunState::InputRequired(Reason::Ready),
            RunState::InputRequired(Reason::TurnEnded),
            RunState::InputRequired(Reason::Asked),
        ] {
            let mut run = at(state);
            assert_eq!(
                run.apply(Cause::Prompted),
                moved(state, RunState::Working, Cause::Prompted)
            );
        }
    }

    #[test]
    fn an_ask_in_the_middle_of_a_turn_is_input_required_asked() {
        let mut run = at(RunState::Working);
        assert_eq!(
            run.apply(Cause::Asked),
            moved(
                RunState::Working,
                RunState::InputRequired(Reason::Asked),
                Cause::Asked
            )
        );
    }

    #[test]
    fn a_nudge_after_the_turn_ended_moves_nothing() {
        let mut run = at(RunState::InputRequired(Reason::TurnEnded));
        assert_eq!(run.apply(Cause::Asked), Ok(None));
        assert_eq!(run.state(), RunState::InputRequired(Reason::TurnEnded));
    }

    #[test]
    fn a_tool_hook_after_an_ask_means_it_was_answered() {
        let mut run = at(RunState::InputRequired(Reason::Asked));
        assert_eq!(
            run.apply(Cause::Answered),
            moved(
                RunState::InputRequired(Reason::Asked),
                RunState::Working,
                Cause::Answered
            )
        );
        assert_eq!(
            run.apply(Cause::Answered),
            Ok(None),
            "a tool hook in a working turn is not a move"
        );
    }

    #[test]
    fn the_turn_ends_from_working_or_from_an_unanswered_ask() {
        for state in [RunState::Working, RunState::InputRequired(Reason::Asked)] {
            let mut run = at(state);
            assert_eq!(
                run.apply(Cause::TurnEnded),
                moved(
                    state,
                    RunState::InputRequired(Reason::TurnEnded),
                    Cause::TurnEnded
                )
            );
        }
    }

    // ----- a hold in place: paused, unpaused ------------------------------------------------

    #[test]
    fn a_budget_policy_or_operator_pauses_a_working_or_waiting_run() {
        for by in [PauseBy::Budget, PauseBy::Policy, PauseBy::Operator] {
            for state in [RunState::Working, RunState::InputRequired(Reason::Asked)] {
                let mut run = at(state);
                assert_eq!(
                    run.apply(Cause::Pause(by)),
                    moved(state, RunState::Paused, Cause::Pause(by))
                );
            }
        }
        assert!(
            at(RunState::Starting)
                .apply(Cause::Pause(PauseBy::Budget))
                .is_err()
        );
    }

    #[test]
    fn unpausing_puts_the_run_back_where_it_was_paused_from() {
        let mut run = at(RunState::InputRequired(Reason::Asked));
        run.apply(Cause::Pause(PauseBy::Operator)).unwrap();
        assert_eq!(
            run.apply(Cause::Unpaused),
            moved(
                RunState::Paused,
                RunState::InputRequired(Reason::Asked),
                Cause::Unpaused
            )
        );
        assert!(at(RunState::Working).apply(Cause::Unpaused).is_err());
    }

    // ----- hibernation: idle, quit, grace ---------------------------------------------------

    #[test]
    fn an_idle_run_at_a_turn_boundary_hibernates_on_a_harness_that_resumes_natively() {
        for reason in [Reason::Ready, Reason::TurnEnded] {
            let mut run = at(RunState::InputRequired(reason));
            assert_eq!(
                run.apply(Cause::Idle),
                moved(
                    RunState::InputRequired(reason),
                    RunState::Hibernated,
                    Cause::Idle
                )
            );
        }
        assert!(
            at(RunState::InputRequired(Reason::Asked))
                .apply(Cause::Idle)
                .is_err(),
            "the ask would be lost with the process"
        );
        assert!(at(RunState::Working).apply(Cause::Idle).is_err());
    }

    #[test]
    fn a_harness_that_cannot_resume_natively_is_kept_hot() {
        let mut run = starting(Facts {
            resumes_natively: false,
            ..TWO
        });
        run.apply(Cause::SessionReported).unwrap();
        assert!(run.apply(Cause::Idle).is_err());
    }

    #[test]
    fn letting_go_of_the_project_hibernates_an_idle_run_and_stops_the_rest() {
        for by in [StopBy::Quit, StopBy::Grace] {
            let mut idle = at(RunState::InputRequired(Reason::TurnEnded));
            assert_eq!(
                idle.apply(Cause::Stop(by)),
                moved(
                    RunState::InputRequired(Reason::TurnEnded),
                    RunState::Hibernated,
                    Cause::Stop(by)
                )
            );
            assert_eq!(idle.apply(Cause::Stop(by)), Ok(None), "hibernated stays so");

            let mut busy = at(RunState::Working);
            assert_eq!(
                busy.apply(Cause::Stop(by)),
                moved(RunState::Working, RunState::Stopped, Cause::Stop(by))
            );

            let mut hot = starting(Facts {
                resumes_natively: false,
                ..TWO
            });
            hot.apply(Cause::SessionReported).unwrap();
            assert_eq!(
                hot.apply(Cause::Stop(by)),
                moved(
                    RunState::InputRequired(Reason::Ready),
                    RunState::Stopped,
                    Cause::Stop(by)
                )
            );
        }
    }

    // ----- ends the operator or the host chose ----------------------------------------------

    #[test]
    fn closing_stopping_or_the_kill_switch_ends_any_live_run_as_stopped() {
        for by in [StopBy::Closed, StopBy::Operator, StopBy::Killed] {
            for state in [
                RunState::Starting,
                RunState::Working,
                RunState::InputRequired(Reason::Ready),
            ] {
                let mut run = at(state);
                assert_eq!(
                    run.apply(Cause::Stop(by)),
                    moved(state, RunState::Stopped, Cause::Stop(by))
                );
            }
            let mut queued = Run::requested(TWO, &[Hold::Launch]);
            assert_eq!(
                queued.apply(Cause::Stop(by)),
                moved(RunState::Queued, RunState::Stopped, Cause::Stop(by))
            );
            let mut asleep = at(RunState::InputRequired(Reason::Ready));
            asleep.apply(Cause::Idle).unwrap();
            assert_eq!(
                asleep.apply(Cause::Stop(by)),
                moved(RunState::Hibernated, RunState::Stopped, Cause::Stop(by))
            );
        }
    }

    #[test]
    fn a_drain_ends_only_a_run_at_its_turn_end() {
        let mut run = at(RunState::InputRequired(Reason::TurnEnded));
        assert_eq!(
            run.apply(Cause::Stop(StopBy::Drained)),
            moved(
                RunState::InputRequired(Reason::TurnEnded),
                RunState::Stopped,
                Cause::Stop(StopBy::Drained)
            )
        );
        assert!(
            at(RunState::Working)
                .apply(Cause::Stop(StopBy::Drained))
                .is_err()
        );
    }

    #[test]
    fn an_exit_after_the_host_signalled_is_a_stop_whatever_the_code() {
        let mut run = at(RunState::Working);
        run.stopping(StopBy::Killed).unwrap();
        assert_eq!(
            run.apply(Cause::Exited(Exit::Signal)),
            moved(
                RunState::Working,
                RunState::Stopped,
                Cause::Stop(StopBy::Killed)
            )
        );
        assert_eq!(run.exit(), Some(Exit::Signal));
    }

    // ----- the next run takes over: superseded, succeeded -----------------------------------

    #[test]
    fn a_run_is_superseded_by_the_next_from_any_live_state() {
        for state in [RunState::Working, RunState::InputRequired(Reason::Ready)] {
            let mut run = at(state);
            assert_eq!(
                run.apply(Cause::Superseded),
                moved(state, RunState::Completed, Cause::Superseded)
            );
        }
        let mut asleep = at(RunState::InputRequired(Reason::Ready));
        asleep.apply(Cause::Idle).unwrap();
        assert_eq!(
            asleep.apply(Cause::Superseded),
            moved(RunState::Hibernated, RunState::Completed, Cause::Superseded)
        );
    }

    #[test]
    fn a_run_succeeding_in_the_same_process_begins_where_its_predecessor_was() {
        let mut previous = at(RunState::Working);
        let (next, ended) = Run::succeeding(&mut previous, TWO).unwrap();
        assert_eq!(next.state(), RunState::Working);
        assert_eq!(
            Ok(Some(ended)),
            moved(RunState::Working, RunState::Completed, Cause::Superseded)
        );
        assert_eq!(previous.state(), RunState::Completed);
    }

    // ----- ends nobody chose: exited, spawn-failed, channel-lost, host-crash ----------------

    #[test]
    fn a_clean_exit_completes_the_run_and_any_other_fails_it() {
        let mut clean = at(RunState::InputRequired(Reason::TurnEnded));
        assert_eq!(
            clean.apply(Cause::Exited(Exit::Code(0))),
            moved(
                RunState::InputRequired(Reason::TurnEnded),
                RunState::Completed,
                Cause::Exited(Exit::Code(0))
            )
        );
        for exit in [Exit::Code(1), Exit::Signal] {
            let mut run = at(RunState::Working);
            assert_eq!(
                run.apply(Cause::Exited(exit)),
                moved(RunState::Working, RunState::Failed, Cause::Exited(exit))
            );
            assert_eq!(run.exit(), Some(exit));
        }
    }

    #[test]
    fn a_lost_exit_code_completes_the_run_only_after_a_session_end() {
        let ended = Exit::Lost {
            session_ended: true,
        };
        let silent = Exit::Lost {
            session_ended: false,
        };
        assert_eq!(
            at(RunState::Working).apply(Cause::Exited(ended)),
            moved(RunState::Working, RunState::Completed, Cause::Exited(ended))
        );
        let mut level_one = starting(Facts {
            level: Level::One,
            ..TWO
        });
        level_one.apply(Cause::Spawned).unwrap();
        assert_eq!(
            level_one.apply(Cause::Exited(silent)),
            moved(RunState::Working, RunState::Failed, Cause::Exited(silent))
        );
        assert_eq!(
            level_one.exit(),
            Some(silent),
            "recorded as lost either way"
        );
    }

    #[test]
    fn a_program_that_could_not_start_fails_the_run() {
        let mut queued = Run::requested(TWO, &[]);
        assert_eq!(
            queued.apply(Cause::SpawnFailed),
            moved(RunState::Queued, RunState::Failed, Cause::SpawnFailed)
        );
        assert_eq!(
            at(RunState::Starting).apply(Cause::SpawnFailed),
            moved(RunState::Starting, RunState::Failed, Cause::SpawnFailed)
        );
    }

    #[test]
    fn a_lost_protocol_channel_fails_only_a_level_three_run() {
        let mut three = starting(Facts {
            level: Level::Three,
            ..TWO
        });
        three.apply(Cause::SessionReported).unwrap();
        three.apply(Cause::Prompted).unwrap();
        assert_eq!(
            three.apply(Cause::ChannelLost),
            moved(RunState::Working, RunState::Failed, Cause::ChannelLost)
        );
        assert!(at(RunState::Working).apply(Cause::ChannelLost).is_err());
    }

    #[test]
    fn a_host_crash_fails_a_busy_run_and_hibernates_an_idle_one() {
        for state in [RunState::Starting, RunState::Working] {
            assert_eq!(
                at(state).apply(Cause::HostCrash),
                moved(state, RunState::Failed, Cause::HostCrash)
            );
        }
        let mut paused = at(RunState::Working);
        paused.apply(Cause::Pause(PauseBy::Budget)).unwrap();
        assert_eq!(
            paused.apply(Cause::HostCrash),
            moved(RunState::Paused, RunState::Failed, Cause::HostCrash)
        );
        assert_eq!(
            at(RunState::InputRequired(Reason::TurnEnded)).apply(Cause::HostCrash),
            moved(
                RunState::InputRequired(Reason::TurnEnded),
                RunState::Hibernated,
                Cause::HostCrash
            )
        );
        assert_eq!(
            at(RunState::InputRequired(Reason::Asked)).apply(Cause::HostCrash),
            moved(
                RunState::InputRequired(Reason::Asked),
                RunState::Failed,
                Cause::HostCrash
            ),
            "the ask was lost"
        );
        let mut queued = Run::requested(TWO, &[]);
        assert_eq!(
            queued.apply(Cause::HostCrash),
            Ok(None),
            "no process to lose"
        );
    }

    // ----- child runs (§6) ------------------------------------------------------------------

    #[test]
    fn a_child_run_is_working_from_first_sight_and_completed_by_its_subagent_stop() {
        let mut child = Run::child(TWO);
        assert_eq!(child.state(), RunState::Working);
        assert!(
            child.apply(Cause::Asked).is_err(),
            "a child's asks are its parent's"
        );
        assert!(child.apply(Cause::Pause(PauseBy::Budget)).is_err());
        assert_eq!(
            child.apply(Cause::ChildEnded),
            moved(RunState::Working, RunState::Completed, Cause::ChildEnded)
        );
        assert!(at(RunState::Working).apply(Cause::ChildEnded).is_err());
    }

    #[test]
    fn a_child_run_ends_with_its_parent_in_the_parents_end_and_for_its_cause() {
        let mut parent = at(RunState::Working);
        let mut child = Run::child(TWO);
        assert_eq!(
            child.end_with_parent(&parent),
            Err(ParentRefusal::ParentLive(RunState::Working))
        );
        parent.apply(Cause::Stop(StopBy::Killed)).unwrap();
        assert_eq!(
            child.end_with_parent(&parent),
            Ok(Move {
                from: RunState::Working,
                to: RunState::Stopped,
                cause: Cause::Stop(StopBy::Killed)
            })
        );
        assert_eq!(
            child.end_with_parent(&parent),
            Err(ParentRefusal::AlreadyEnded(RunState::Stopped))
        );
        assert_eq!(
            at(RunState::Working).end_with_parent(&parent),
            Err(ParentRefusal::NotAChild)
        );
    }

    // ----- remote chats (§7) ----------------------------------------------------------------

    #[test]
    fn a_remote_chats_run_moves_only_by_what_its_vendor_reports() {
        let mut remote = Run::observed(RunState::Working, TWO).unwrap();
        assert_eq!(remote.state(), RunState::Working);
        for cause in [
            Cause::Pause(PauseBy::Budget),
            Cause::Stop(StopBy::Killed),
            Cause::Idle,
            Cause::TurnEnded,
        ] {
            assert!(
                remote.apply(cause).is_err(),
                "{cause:?} is not charter's to cause"
            );
        }
        let done = Cause::Observed(RunState::Completed);
        assert_eq!(
            remote.apply(done),
            moved(RunState::Working, RunState::Completed, done)
        );
        assert!(
            at(RunState::Working)
                .apply(Cause::Observed(RunState::Completed))
                .is_err(),
            "a governed run is never moved by a vendor report"
        );
    }

    // ----- needs you (§9) -------------------------------------------------------------------

    #[test]
    fn needs_you_holds_an_ask_a_finished_turn_a_budget_or_policy_pause_and_a_spent_budget() {
        assert!(at(RunState::InputRequired(Reason::Asked)).needs_you());
        assert!(at(RunState::InputRequired(Reason::TurnEnded)).needs_you());
        assert!(!at(RunState::InputRequired(Reason::Ready)).needs_you());
        assert!(!at(RunState::Working).needs_you());
        for (by, needs) in [
            (PauseBy::Budget, true),
            (PauseBy::Policy, true),
            (PauseBy::Operator, false),
        ] {
            let mut run = at(RunState::Working);
            run.apply(Cause::Pause(by)).unwrap();
            assert_eq!(run.needs_you(), needs, "{by:?}");
        }
        assert!(Run::requested(TWO, &[Hold::Budget]).needs_you());
        assert!(!Run::requested(TWO, &[Hold::Stagger]).needs_you());
    }

    #[test]
    fn a_finished_or_failed_run_needs_you_until_the_chat_is_shown_and_a_stop_never_does() {
        for exit in [Exit::Code(0), Exit::Code(2)] {
            let mut run = at(RunState::Working);
            run.apply(Cause::Exited(exit)).unwrap();
            assert!(run.needs_you(), "{exit:?}");
            run.shown();
            assert!(!run.needs_you());
        }
        let mut crashed = at(RunState::Working);
        crashed.apply(Cause::HostCrash).unwrap();
        assert!(crashed.needs_you());

        let mut stopped = at(RunState::InputRequired(Reason::TurnEnded));
        stopped.apply(Cause::Stop(StopBy::Closed)).unwrap();
        assert!(!stopped.needs_you());

        let mut superseded = at(RunState::InputRequired(Reason::TurnEnded));
        superseded.apply(Cause::Superseded).unwrap();
        assert!(!superseded.needs_you(), "its chat carries on");
    }

    // ----- the rule for no-move against refusal ---------------------------------------------

    #[test]
    fn a_fact_the_harness_sends_routinely_moves_nothing_and_an_impossible_one_is_refused() {
        // Routine: an idle nudge at a fresh session, a tool hook in a working turn.
        assert_eq!(
            at(RunState::InputRequired(Reason::Ready)).apply(Cause::Asked),
            Ok(None)
        );
        assert_eq!(at(RunState::Working).apply(Cause::Answered), Ok(None));
        // Impossible here: the table has no row, and nothing sends it routinely.
        for cause in [Cause::TurnEnded, Cause::Answered] {
            assert!(
                at(RunState::InputRequired(Reason::Ready))
                    .apply(cause)
                    .is_err(),
                "{cause:?} from ready"
            );
        }
        let mut paused = at(RunState::Working);
        paused.apply(Cause::Pause(PauseBy::Operator)).unwrap();
        assert!(paused.apply(Cause::Pause(PauseBy::Budget)).is_err());
        assert!(
            Run::requested(TWO, &[Hold::Disk])
                .apply(Cause::Admitted)
                .is_err(),
            "a held run is not admitted"
        );
    }

    // ----- the stop intent goes through the same table as a stop ----------------------------

    #[test]
    fn an_exit_after_a_quit_or_grace_intent_hibernates_an_idle_run() {
        for by in [StopBy::Quit, StopBy::Grace] {
            let mut run = at(RunState::InputRequired(Reason::TurnEnded));
            run.stopping(by).unwrap();
            assert_eq!(
                run.apply(Cause::Exited(Exit::Signal)),
                moved(
                    RunState::InputRequired(Reason::TurnEnded),
                    RunState::Hibernated,
                    Cause::Stop(by)
                )
            );
            assert_eq!(run.exit(), Some(Exit::Signal));
        }
    }

    #[test]
    fn an_exit_after_a_quit_intent_stops_a_busy_run() {
        let mut run = at(RunState::Working);
        run.stopping(StopBy::Quit).unwrap();
        assert_eq!(
            run.apply(Cause::Exited(Exit::Code(130))),
            moved(
                RunState::Working,
                RunState::Stopped,
                Cause::Stop(StopBy::Quit)
            )
        );
        assert_eq!(run.exit(), Some(Exit::Code(130)));
    }

    #[test]
    fn a_drain_intent_is_taken_only_at_input_required() {
        assert!(at(RunState::Working).stopping(StopBy::Drained).is_err());
        let mut run = at(RunState::InputRequired(Reason::TurnEnded));
        run.stopping(StopBy::Drained).unwrap();
        assert_eq!(
            run.apply(Cause::Exited(Exit::Code(0))),
            moved(
                RunState::InputRequired(Reason::TurnEnded),
                RunState::Stopped,
                Cause::Stop(StopBy::Drained)
            )
        );
    }

    #[test]
    fn a_stop_intent_needs_a_program_to_stop() {
        assert!(Run::requested(TWO, &[]).stopping(StopBy::Killed).is_err());
        let mut asleep = at(RunState::InputRequired(Reason::Ready));
        asleep.apply(Cause::Idle).unwrap();
        assert!(asleep.stopping(StopBy::Killed).is_err());
    }

    #[test]
    fn the_exit_after_a_direct_stop_or_hibernation_is_recorded_not_refused() {
        let mut stopped = at(RunState::Working);
        stopped.apply(Cause::Stop(StopBy::Killed)).unwrap();
        assert_eq!(stopped.apply(Cause::Exited(Exit::Signal)), Ok(None));
        assert_eq!(stopped.exit(), Some(Exit::Signal));
        assert!(
            stopped.apply(Cause::Exited(Exit::Signal)).is_err(),
            "one exit per program"
        );

        let mut asleep = at(RunState::InputRequired(Reason::TurnEnded));
        asleep.apply(Cause::Idle).unwrap();
        assert_eq!(asleep.apply(Cause::Exited(Exit::Code(0))), Ok(None));
        assert_eq!(asleep.exit(), Some(Exit::Code(0)));
        assert_eq!(asleep.state(), RunState::Hibernated);

        let mut quit = at(RunState::InputRequired(Reason::Ready));
        quit.apply(Cause::Stop(StopBy::Quit)).unwrap();
        assert_eq!(quit.apply(Cause::Exited(Exit::Signal)), Ok(None));
        assert_eq!(quit.exit(), Some(Exit::Signal));
    }

    #[test]
    fn a_run_that_never_had_a_program_takes_no_exit() {
        let mut refused = Run::requested(TWO, &[]);
        refused.apply(Cause::Refused(Gate::KillSwitch)).unwrap();
        assert!(refused.apply(Cause::Exited(Exit::Code(0))).is_err());
    }

    // ----- succeeding needs a live process --------------------------------------------------

    #[test]
    fn a_successor_in_the_same_process_needs_a_predecessor_with_a_live_process() {
        for state in [
            RunState::Starting,
            RunState::Working,
            RunState::InputRequired(Reason::Ready),
        ] {
            let mut previous = at(state);
            let (next, _) = Run::succeeding(&mut previous, TWO).unwrap();
            assert_eq!(next.state(), state);
        }
        let mut paused = at(RunState::Working);
        paused.apply(Cause::Pause(PauseBy::Operator)).unwrap();
        let (mut next, _) = Run::succeeding(&mut paused, TWO).unwrap();
        assert_eq!(next.state(), RunState::Paused);
        assert_eq!(
            next.apply(Cause::Unpaused),
            moved(RunState::Paused, RunState::Working, Cause::Unpaused)
        );

        let mut queued = Run::requested(TWO, &[]);
        let mut asleep = at(RunState::InputRequired(Reason::Ready));
        asleep.apply(Cause::Idle).unwrap();
        let mut ended = at(RunState::Working);
        ended.apply(Cause::Exited(Exit::Code(0))).unwrap();
        for previous in [&mut queued, &mut asleep, &mut ended] {
            let before = previous.state();
            assert!(Run::succeeding(previous, TWO).is_err(), "{before:?}");
            assert_eq!(previous.state(), before, "a refusal moves nothing");
        }
    }

    #[test]
    fn a_queued_or_paused_run_is_superseded_by_a_run_in_a_new_process() {
        let mut queued = Run::requested(TWO, &[Hold::Stagger]);
        assert_eq!(
            queued.apply(Cause::Superseded),
            moved(RunState::Queued, RunState::Completed, Cause::Superseded)
        );
        let mut paused = at(RunState::Working);
        paused.apply(Cause::Pause(PauseBy::Budget)).unwrap();
        assert_eq!(
            paused.apply(Cause::Superseded),
            moved(RunState::Paused, RunState::Completed, Cause::Superseded)
        );
    }

    // ----- observed: only the states a vendor can report ------------------------------------

    #[test]
    fn a_remote_chats_run_is_never_queued_starting_paused_or_hibernated() {
        for state in [
            RunState::Queued,
            RunState::Starting,
            RunState::Paused,
            RunState::Hibernated,
        ] {
            assert!(Run::observed(state, TWO).is_err(), "{state:?}");
            let mut remote = Run::observed(RunState::Working, TWO).unwrap();
            assert!(remote.apply(Cause::Observed(state)).is_err(), "{state:?}");
        }
        let mut remote = Run::observed(RunState::Working, TWO).unwrap();
        assert!(
            remote.apply(Cause::Observed(RunState::Working)).is_err(),
            "a report of where it already is is not a move"
        );
        assert!(Run::observed(RunState::Completed, TWO).is_ok());
    }

    // ----- rows the review found untested ---------------------------------------------------

    #[test]
    fn a_paused_programs_exit_completes_or_fails_the_run_by_its_code() {
        for (exit, to) in [
            (Exit::Code(0), RunState::Completed),
            (Exit::Code(1), RunState::Failed),
        ] {
            let mut run = at(RunState::Working);
            run.apply(Cause::Pause(PauseBy::Operator)).unwrap();
            assert_eq!(
                run.apply(Cause::Exited(exit)),
                moved(RunState::Paused, to, Cause::Exited(exit))
            );
        }
    }

    #[test]
    fn a_host_crash_fails_an_idle_run_on_a_harness_that_cannot_resume() {
        let mut run = starting(Facts {
            resumes_natively: false,
            ..TWO
        });
        run.apply(Cause::SessionReported).unwrap();
        assert_eq!(
            run.apply(Cause::HostCrash),
            moved(
                RunState::InputRequired(Reason::Ready),
                RunState::Failed,
                Cause::HostCrash
            )
        );
    }

    #[test]
    fn a_host_crash_leaves_a_hibernated_run_as_it_is() {
        let mut run = at(RunState::InputRequired(Reason::Ready));
        run.apply(Cause::Idle).unwrap();
        assert_eq!(run.apply(Cause::HostCrash), Ok(None));
        assert_eq!(run.state(), RunState::Hibernated);
    }

    // ----- every row of ADR 0076's tables ---------------------------------------------------

    /// One row of a transition table, as the ADR writes its first three cells.
    struct Row {
        from: String,
        to: String,
        cause: String,
    }

    const ADR: &str = include_str!(
        "../../../../docs/adr/0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md"
    );

    /// The rows of the first `| From | To | Cause |` table under `heading`. Fails loudly when the
    /// heading or the table is missing, so a renamed section cannot make a test check nothing.
    fn table(heading: &str) -> Vec<Row> {
        let section = ADR
            .split_once(heading)
            .unwrap_or_else(|| panic!("ADR 0076 has no {heading:?}"))
            .1;
        let mut lines = section
            .lines()
            .skip_while(|line| !line.starts_with("| From | To | Cause |"));
        assert!(
            lines.next().is_some(),
            "no transition table under {heading:?}"
        );
        assert!(
            lines.next().is_some_and(|line| line.starts_with("|---")),
            "the table under {heading:?} has no separator"
        );
        let rows: Vec<Row> = lines
            .take_while(|line| line.starts_with('|'))
            .map(|line| {
                // A `\|` inside a cell is a pipe the cell holds, not a column.
                let line = line.replace("\\|", "\u{1}");
                let cells: Vec<String> = line
                    .trim_matches('|')
                    .split('|')
                    .map(|cell| cell.trim().replace('\u{1}', "|"))
                    .collect();
                assert!(cells.len() >= 3, "a short row: {line}");
                Row {
                    from: cells[0].clone(),
                    to: cells[1].clone(),
                    cause: cells[2].trim_matches('`').to_owned(),
                }
            })
            .collect();
        assert!(!rows.is_empty(), "an empty table under {heading:?}");
        rows
    }

    /// The states a state name covers, as the tables write it.
    fn state_named(name: &str) -> Vec<RunState> {
        match name.trim().trim_matches('`') {
            "queued" => vec![RunState::Queued],
            "starting" => vec![RunState::Starting],
            "working" => vec![RunState::Working],
            "paused" => vec![RunState::Paused],
            "hibernated" => vec![RunState::Hibernated],
            "completed" => vec![RunState::Completed],
            "failed" => vec![RunState::Failed],
            "stopped" => vec![RunState::Stopped],
            "input-required" => vec![
                RunState::InputRequired(Reason::Asked),
                RunState::InputRequired(Reason::TurnEnded),
                RunState::InputRequired(Reason::Ready),
            ],
            "input-required (asked)" => vec![RunState::InputRequired(Reason::Asked)],
            "input-required (ready)" => vec![RunState::InputRequired(Reason::Ready)],
            "input-required (turn-ended)" => vec![RunState::InputRequired(Reason::TurnEnded)],
            "input-required (ready or turn-ended)" => vec![
                RunState::InputRequired(Reason::Ready),
                RunState::InputRequired(Reason::TurnEnded),
            ],
            other => panic!("ADR 0076 names a state this test cannot read: {other:?}"),
        }
    }

    /// The states a `From` cell names.
    fn from_cell(cell: &str) -> Vec<RunState> {
        let live = [
            RunState::Queued,
            RunState::Starting,
            RunState::Working,
            RunState::InputRequired(Reason::Asked),
            RunState::InputRequired(Reason::TurnEnded),
            RunState::InputRequired(Reason::Ready),
            RunState::Paused,
            RunState::Hibernated,
        ];
        match cell {
            "any live state" => live.to_vec(),
            "any live state but `hibernated`" => live
                .into_iter()
                .filter(|state| *state != RunState::Hibernated)
                .collect(),
            cells => cells.split(", ").flat_map(state_named).collect(),
        }
    }

    /// A run of `facts` in `state`, reached only through moves.
    fn reach(state: RunState, facts: Facts) -> Run {
        let mut run = Run::requested(facts, &[]);
        let path: &[Cause] = match state {
            RunState::Queued => &[],
            RunState::Starting => &[Cause::Admitted],
            RunState::Working if facts.level == Level::One => &[Cause::Admitted, Cause::Spawned],
            RunState::Working => &[Cause::Admitted, Cause::SessionReported, Cause::Prompted],
            RunState::InputRequired(Reason::Ready) => &[Cause::Admitted, Cause::SessionReported],
            RunState::InputRequired(Reason::Asked) => &[
                Cause::Admitted,
                Cause::SessionReported,
                Cause::Prompted,
                Cause::Asked,
            ],
            RunState::InputRequired(Reason::TurnEnded) => &[
                Cause::Admitted,
                Cause::SessionReported,
                Cause::Prompted,
                Cause::TurnEnded,
            ],
            RunState::Paused => &[
                Cause::Admitted,
                Cause::SessionReported,
                Cause::Prompted,
                Cause::Pause(PauseBy::Budget),
            ],
            RunState::Hibernated => &[Cause::Admitted, Cause::SessionReported, Cause::Idle],
            end => panic!("no run is reached in {end:?} to move from"),
        };
        for cause in path {
            run.apply(*cause)
                .unwrap_or_else(|refusal| panic!("reaching {state:?}: {refusal:?}"));
        }
        assert_eq!(run.state(), state);
        run
    }

    /// Each cause's place in [`EVERY_CAUSE`], by an exhaustive `match`. A new variant fails to
    /// compile until it is given a place here, and then fails
    /// [`every_cause_the_adr_names_is_one_this_module_moves_by_and_no_other`] until it is listed
    /// and the ADR names it.
    fn index(cause: Cause) -> usize {
        match cause {
            Cause::Admitted => 0,
            Cause::Refused(_) => 1,
            Cause::SessionReported => 2,
            Cause::Spawned => 3,
            Cause::Prompted => 4,
            Cause::Answered => 5,
            Cause::Asked => 6,
            Cause::TurnEnded => 7,
            Cause::Pause(PauseBy::Budget) => 8,
            Cause::Pause(PauseBy::Policy) => 9,
            Cause::Pause(PauseBy::Operator) => 10,
            Cause::Unpaused => 11,
            Cause::Idle => 12,
            Cause::Superseded => 13,
            Cause::Exited(_) => 14,
            Cause::SpawnFailed => 15,
            Cause::ChannelLost => 16,
            Cause::HostCrash => 17,
            Cause::Stop(StopBy::Closed) => 18,
            Cause::Stop(StopBy::Operator) => 19,
            Cause::Stop(StopBy::Killed) => 20,
            Cause::Stop(StopBy::Quit) => 21,
            Cause::Stop(StopBy::Grace) => 22,
            Cause::Stop(StopBy::Drained) => 23,
            Cause::ChildEnded => 24,
            Cause::Observed(_) => 25,
        }
    }

    const EVERY_CAUSE: [Cause; 26] = [
        Cause::Admitted,
        Cause::Refused(Gate::KillSwitch),
        Cause::SessionReported,
        Cause::Spawned,
        Cause::Prompted,
        Cause::Answered,
        Cause::Asked,
        Cause::TurnEnded,
        Cause::Pause(PauseBy::Budget),
        Cause::Pause(PauseBy::Policy),
        Cause::Pause(PauseBy::Operator),
        Cause::Unpaused,
        Cause::Idle,
        Cause::Superseded,
        Cause::Exited(Exit::Code(0)),
        Cause::SpawnFailed,
        Cause::ChannelLost,
        Cause::HostCrash,
        Cause::Stop(StopBy::Closed),
        Cause::Stop(StopBy::Operator),
        Cause::Stop(StopBy::Killed),
        Cause::Stop(StopBy::Quit),
        Cause::Stop(StopBy::Grace),
        Cause::Stop(StopBy::Drained),
        Cause::ChildEnded,
        Cause::Observed(RunState::Working),
    ];

    /// The cause a row's word and target stand for, and the facts its prose assumes: a level-1
    /// harness for `spawned` into `working`, level 3 for `channel-lost`, and a harness that
    /// cannot resume natively where an idle run is failed or stopped rather than hibernated.
    fn cause_for(word: &str, from: RunState, to: RunState) -> (Cause, Facts) {
        let hot = Facts {
            resumes_natively: false,
            ..TWO
        };
        let kept_hot = matches!(from, RunState::InputRequired(_)) && to != RunState::Hibernated;
        let facts = if kept_hot { hot } else { TWO };
        match word {
            "admitted" => (Cause::Admitted, facts),
            "refused" => (Cause::Refused(Gate::KillSwitch), facts),
            "session-reported" => (Cause::SessionReported, facts),
            "spawned" if to == RunState::Working => (
                Cause::Spawned,
                Facts {
                    level: Level::One,
                    ..TWO
                },
            ),
            "spawned" => (
                Cause::Spawned,
                Facts {
                    reports_start: false,
                    ..TWO
                },
            ),
            "prompted" => (Cause::Prompted, facts),
            "answered" => (Cause::Answered, facts),
            "asked" => (Cause::Asked, facts),
            "turn-ended" => (Cause::TurnEnded, facts),
            "budget" => (Cause::Pause(PauseBy::Budget), facts),
            "policy" => (Cause::Pause(PauseBy::Policy), facts),
            "operator" if to == RunState::Paused => (Cause::Pause(PauseBy::Operator), facts),
            "operator" => (Cause::Stop(StopBy::Operator), facts),
            "unpaused" => (Cause::Unpaused, facts),
            "idle" => (Cause::Idle, facts),
            "superseded" => (Cause::Superseded, facts),
            "exited" if to == RunState::Completed => (Cause::Exited(Exit::Code(0)), facts),
            "exited" => (Cause::Exited(Exit::Code(1)), facts),
            "spawn-failed" => (Cause::SpawnFailed, facts),
            "channel-lost" => (
                Cause::ChannelLost,
                Facts {
                    level: Level::Three,
                    ..TWO
                },
            ),
            "host-crash" => (Cause::HostCrash, facts),
            "closed" => (Cause::Stop(StopBy::Closed), facts),
            "killed" => (Cause::Stop(StopBy::Killed), facts),
            "quit" => (Cause::Stop(StopBy::Quit), facts),
            "grace" => (Cause::Stop(StopBy::Grace), facts),
            "drained" => (Cause::Stop(StopBy::Drained), facts),
            other => panic!("ADR 0076 names a cause this test cannot build: {other:?}"),
        }
    }

    #[test]
    fn every_row_of_the_adrs_transition_table_is_a_move_this_module_makes() {
        let rows = table("### 2. Every move names one cause");
        for row in &rows {
            match (row.from.as_str(), row.cause.as_str()) {
                ("(none)", "requested") => {
                    assert_eq!(Run::requested(TWO, &[]).state(), RunState::Queued);
                }
                ("(none)", "succeeded") => {
                    let mut previous = reach(RunState::Working, TWO);
                    let (next, _) = Run::succeeding(&mut previous, TWO).unwrap();
                    assert_eq!(next.state(), RunState::Working);
                }
                (from, word) => {
                    for from in from_cell(from) {
                        let tos = match row.to.as_str() {
                            // Paused from `working` on the way here (`reach`).
                            "the state it was paused from" => vec![RunState::Working],
                            to => state_named(to),
                        };
                        for to in tos {
                            let (cause, facts) = cause_for(word, from, to);
                            let mut run = reach(from, facts);
                            assert_eq!(
                                run.apply(cause),
                                Ok(Some(Move { from, to, cause })),
                                "row `{} | {} | {}`",
                                row.from,
                                row.to,
                                row.cause
                            );
                        }
                    }
                }
            }
        }
        assert!(rows.len() >= 30, "only {} rows read", rows.len());
    }

    #[test]
    fn every_row_of_the_adrs_child_table_is_a_move_this_module_makes() {
        let rows = table("### 6. A child run's states");
        let words: Vec<&str> = rows.iter().map(|row| row.cause.as_str()).collect();
        assert_eq!(
            words,
            ["child-seen", "child-ended", "the parent's cause"],
            "§6's rows changed"
        );
        assert_eq!(Run::child(TWO).state(), RunState::Working);
        assert_eq!(
            Run::child(TWO).apply(Cause::ChildEnded),
            moved(RunState::Working, RunState::Completed, Cause::ChildEnded)
        );
        let mut parent = reach(RunState::Working, TWO);
        parent.apply(Cause::Exited(Exit::Code(1))).unwrap();
        assert_eq!(
            Run::child(TWO).end_with_parent(&parent),
            Ok(Move {
                from: RunState::Working,
                to: RunState::Failed,
                cause: Cause::Exited(Exit::Code(1))
            })
        );
    }

    #[test]
    fn every_cause_the_adr_names_is_one_this_module_moves_by_and_no_other() {
        let places: BTreeSet<usize> = EVERY_CAUSE.iter().map(|cause| index(*cause)).collect();
        assert_eq!(
            places,
            (0..EVERY_CAUSE.len()).collect(),
            "EVERY_CAUSE holds one of each cause, and nothing twice"
        );
        let mut ours: BTreeSet<&str> = EVERY_CAUSE.iter().map(|cause| cause.word()).collect();
        // The three ways a run begins, which are not moves of an existing run.
        ours.extend(["requested", "succeeded", "child-seen"]);
        // A remote chat's one cause is named in §7's prose, not in a table.
        assert!(ours.remove("observed"));
        assert!(ADR.contains("the one cause `observed`"));
        let rows = table("### 2. Every move names one cause")
            .into_iter()
            .chain(table("### 6. A child run's states"));
        let theirs: BTreeSet<String> = rows
            .map(|row| row.cause)
            .filter(|cause| cause != "the parent's cause")
            .collect();
        assert_eq!(ours, theirs.iter().map(String::as_str).collect());
    }
}
