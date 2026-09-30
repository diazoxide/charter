//! A run's state, and the only way it moves (ADR 0076).
//!
//! A run is in one of nine states, and every move names exactly one [`Cause`]: a fact from a
//! hook, the harness's protocol, the program's exit, or an act of the host, the operator or a
//! policy. Nothing here reads a harness's output. The three end states are final.

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
    /// How the program ended, once it has.
    exit: Option<Exit>,
    /// The cause of the move that ended the run, once it has ended.
    ended_by: Option<Cause>,
    /// Whether the operator has been shown the chat since the run ended.
    seen: bool,
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
    /// first reports it.
    pub fn observed(state: RunState, facts: Facts) -> Self {
        Self {
            kind: Kind::Observed,
            state,
            ..Run::requested(facts, &[])
        }
    }

    /// A child run's parent has ended: the child ends with it, in the parent's end state and
    /// for the parent's cause.
    pub fn end_with_parent(&mut self, parent: &Run) -> Result<Option<Move>, Refusal> {
        let from = self.state;
        match (self.kind, parent.ended_by) {
            (Kind::Child, Some(cause)) if !from.is_end() => {
                Ok(Some(self.moved(from, parent.state, cause)))
            }
            (_, cause) => Err(Refusal {
                state: from,
                cause: cause.unwrap_or(Cause::ChildEnded),
            }),
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

    /// Makes the move, and remembers the cause that ended the run if it did.
    fn moved(&mut self, from: RunState, to: RunState, cause: Cause) -> Move {
        self.state = to;
        if to.is_end() {
            self.ended_by = Some(cause);
        }
        Move { from, to, cause }
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

    /// A new run beginning in the same process as `previous` (`clear`, or a `switch` that keeps
    /// the process): `(none) → <previous's state> | succeeded`. `previous` ends `completed |
    /// superseded` in the same step, and that move is answered beside the new run.
    pub fn succeeding(previous: &mut Run, facts: Facts) -> (Run, Result<Option<Move>, Refusal>) {
        let mut next = Run::requested(facts, &[]);
        next.state = previous.state;
        next.paused = previous.paused;
        let ended = previous.apply(Cause::Superseded);
        (next, ended)
    }

    /// The host is about to signal this run's program to end it, for `by`. Not a move: the
    /// exit that follows is.
    pub fn stopping(&mut self, by: StopBy) {
        self.stopping = Some(by);
    }

    /// How the run's program ended, once it has.
    pub fn exit(&self) -> Option<Exit> {
        self.exit
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

    /// `cause` happened. Answers the move it made, `None` when the cause is one this state
    /// takes without moving, or a [`Refusal`] when it may not come from here at all.
    pub fn apply(&mut self, cause: Cause) -> Result<Option<Move>, Refusal> {
        let from = self.state;
        let refuse = Refusal { state: from, cause };
        if from.is_end() {
            return Err(refuse);
        }
        // A child run and a remote chat's run each take one cause only.
        match (self.kind, cause) {
            (Kind::Child, Cause::ChildEnded) => {
                return Ok(Some(self.moved(from, RunState::Completed, cause)));
            }
            (Kind::Observed, Cause::Observed(to)) => return Ok(Some(self.moved(from, to, cause))),
            (Kind::Child | Kind::Observed, _)
            | (Kind::Governed, Cause::ChildEnded | Cause::Observed(_)) => {
                return Err(refuse);
            }
            (Kind::Governed, _) => {}
        }
        // The one cause whose move depends on something besides the state: an exit after the
        // host said it would stop the run is that stop, whatever the code says.
        let mut cause = cause;
        let to = match (from, cause) {
            (RunState::Queued, Cause::Admitted) if self.holds.is_empty() => RunState::Starting,
            (RunState::Queued, Cause::Refused(_)) => RunState::Stopped,
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
                // It will report its own start; the spawn is not the fact that moves it.
                _ => return Ok(None),
            },
            (RunState::Starting | RunState::InputRequired(_), Cause::Prompted) => RunState::Working,
            (RunState::InputRequired(Reason::Asked), Cause::Answered) => RunState::Working,
            // A tool hook in a turn that is going on anyway.
            (RunState::Working | RunState::InputRequired(_), Cause::Answered) => return Ok(None),
            (RunState::Working, Cause::Asked) => RunState::InputRequired(Reason::Asked),
            // Claude Code's idle nudge, or an ask repeated: nothing new.
            (RunState::InputRequired(_), Cause::Asked) => return Ok(None),
            (RunState::Working | RunState::InputRequired(Reason::Asked), Cause::TurnEnded) => {
                RunState::InputRequired(Reason::TurnEnded)
            }
            (RunState::InputRequired(_), Cause::TurnEnded) => return Ok(None),

            (RunState::Working | RunState::InputRequired(_), Cause::Pause(by)) => {
                self.paused = Some((from, by));
                RunState::Paused
            }
            (RunState::Paused, Cause::Unpaused) => match self.paused.take() {
                Some((back, _)) => back,
                None => return Err(refuse),
            },

            (RunState::InputRequired(_), Cause::Idle) if self.can_hibernate() => {
                RunState::Hibernated
            }
            (RunState::Hibernated, Cause::Stop(StopBy::Quit | StopBy::Grace)) => return Ok(None),
            (_, Cause::Stop(StopBy::Quit | StopBy::Grace)) if self.can_hibernate() => {
                RunState::Hibernated
            }
            (RunState::InputRequired(_), Cause::Stop(StopBy::Drained)) => RunState::Stopped,
            (_, Cause::Stop(StopBy::Drained)) => return Err(refuse),
            (_, Cause::Stop(_)) => RunState::Stopped,

            (_, Cause::Superseded) => RunState::Completed,

            (
                RunState::Starting
                | RunState::Working
                | RunState::InputRequired(_)
                | RunState::Paused,
                Cause::Exited(exit),
            ) => {
                self.exit = Some(exit);
                if let Some(by) = self.stopping {
                    cause = Cause::Stop(by);
                    RunState::Stopped
                } else {
                    match exit {
                        Exit::Code(0)
                        | Exit::Lost {
                            session_ended: true,
                        } => RunState::Completed,
                        Exit::Code(_) | Exit::Signal | Exit::Lost { .. } => RunState::Failed,
                    }
                }
            }
            (RunState::Queued | RunState::Starting, Cause::SpawnFailed) => RunState::Failed,
            (RunState::Working | RunState::InputRequired(_), Cause::ChannelLost)
                if self.facts.level == Level::Three =>
            {
                RunState::Failed
            }

            (RunState::Queued | RunState::Hibernated, Cause::HostCrash) => return Ok(None),
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
            _ => return Err(refuse),
        };
        Ok(Some(self.moved(from, to, cause)))
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
        run.stopping(StopBy::Killed);
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
        let (next, ended) = Run::succeeding(&mut previous, TWO);
        assert_eq!(next.state(), RunState::Working);
        assert_eq!(
            ended,
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
        assert!(
            child.end_with_parent(&parent).is_err(),
            "the parent is still live"
        );
        parent.apply(Cause::Stop(StopBy::Killed)).unwrap();
        assert_eq!(
            child.end_with_parent(&parent),
            moved(
                RunState::Working,
                RunState::Stopped,
                Cause::Stop(StopBy::Killed)
            )
        );
    }

    // ----- remote chats (§7) ----------------------------------------------------------------

    #[test]
    fn a_remote_chats_run_moves_only_by_what_its_vendor_reports() {
        let mut remote = Run::observed(RunState::Working, TWO);
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

    // ----- the list of causes is ADR 0076's ---------------------------------------------------

    /// Every cause word ADR 0076's transition tables (§2 and §6) name, in the third column.
    fn causes_the_adr_names() -> BTreeSet<String> {
        let adr = include_str!(
            "../../../../docs/adr/0076-a-run-moves-only-by-a-named-cause-and-a-chats-state-is-read-from-its-runs.md"
        );
        let mut words = BTreeSet::new();
        for line in adr.lines().filter(|line| line.starts_with("| ")) {
            let cells: Vec<&str> = line.split(" | ").collect();
            let Some(cell) = cells.get(2) else { continue };
            if let Some(word) = cell.strip_prefix('`').and_then(|w| w.strip_suffix('`'))
                && !word.contains(' ')
            {
                words.insert(word.to_owned());
            }
        }
        words
    }

    #[test]
    fn every_cause_the_adr_names_is_one_this_module_moves_by_and_no_other() {
        let every = [
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
        ];
        let mut ours: BTreeSet<String> = every.iter().map(|c| c.word().to_owned()).collect();
        // The three ways a run begins, which are not moves of an existing run.
        ours.extend(["requested", "succeeded", "child-seen"].map(str::to_owned));
        assert_eq!(ours, causes_the_adr_names());
        assert_eq!(Cause::Observed(RunState::Working).word(), "observed");
    }
}
