//! **The live ask** (#1666, spec #1661, decisions N-2, N-6, N-10): a chat's connection to a host
//! nothing lists is *held* while the person is asked, and goes on the moment they allow it, so
//! the command that made it carries on with nothing failing and nothing restarting.
//!
//! One [`Asks`] board stands beside each chat's proxy ([`super::egress`]), made when the chat
//! starts and gone when it ends. The proxy asks it of a connection whose decision is
//! [`super::reach::Decision::Ask`] ([`Asks::hold`]); the app answers it from the window alone
//! ([`Asks::allow`], [`Asks::keep_blocked`]), and nothing a connection carries is ever read as
//! an answer.
//!
//! - **Held, then allowed.** A held connection waits up to [`HOLD`]. An Allow at any scope wakes
//!   it, and it is carried as an Allowed host at that scope.
//! - **Held, then timed out.** Nobody answered within [`HOLD`]: the connection is refused as it
//!   was before purlis asked live, the timeout is told ([`Heard::TimedOut`]) so it is recorded,
//!   and the ask stays with the person. A later Allow applies at once, and answers the hosts that
//!   gave up, so the app tells the chat, in fixed words, to retry.
//! - **Grouped.** New hosts one chat reaches within [`GROUP`] of the first are one ask
//!   ([`Heard::Asked`]): told together once the window has passed, each host and port named
//!   whole as the proxy heard it, never a wildcard, so the person answers once.
//! - **Keep blocked** refuses what is held on that host now and what comes after, until an Allow.
//! - **Bounded.** At most [`MOST_HELD`] connections are held at once; one more is refused at
//!   once, as before, and not asked about ([`Answer::Busy`]).

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::egress::{allows_on, host_and_port};
use super::hosts::Host;
use super::reach::By;

/// How long a held connection waits for the person: about a minute (N-6).
pub const HOLD: Duration = Duration::from_secs(60);

/// How close together new hosts from one chat must come to be one ask (N-10).
pub const GROUP: Duration = Duration::from_secs(2);

/// The most connections one chat's board holds at once.
pub const MOST_HELD: usize = 16;

/// The most hosts one board remembers as given up on, kept blocked, or allowed live.
const MOST_KEPT: usize = 64;

/// What a held connection is answered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The person allowed it, at this scope: carried.
    Allowed(By),
    /// The person kept it blocked: refused.
    Refused,
    /// Nobody answered in time: refused, and the ask stays.
    TimedOut,
    /// The board holds all it holds: refused at once, not asked about.
    Busy,
}

/// What a board tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// Ask the person about these hosts, each `host:port` whole, in the order they came: one
    /// Notice, one decision.
    Asked(Vec<String>),
    /// A connection to `host:port` was held and nobody answered: it was refused.
    TimedOut(String),
}

/// Who hears what a board tells.
pub type Tell = Arc<dyn Fn(Heard) + Send + Sync + 'static>;

#[derive(Default)]
struct State {
    /// Hosts allowed while the chat runs, each with its scope, as a grant writes them.
    live: Vec<(String, By)>,
    /// Hosts kept blocked, until an Allow.
    blocked: Vec<Host>,
    /// Targets held and given up on, which an Allow tells the chat to retry.
    gave_up: Vec<String>,
    /// Targets asked about and not answered yet, so each is asked once.
    asked: Vec<String>,
    /// The ask being gathered: its targets, and when it is told.
    gathering: Option<(Vec<String>, Instant)>,
    holding: usize,
}

/// **One chat's live asks** (module docs).
pub struct Asks {
    state: Mutex<State>,
    changed: Condvar,
    tell: Tell,
    hold: Duration,
    group: Duration,
    known: Option<Known>,
}

/// Where a host the person allowed already, since the chat started, is found ([`Asks::knowing`]):
/// the scope it is allowed at, or none.
pub type Known = Arc<dyn Fn(&Host) -> Option<By> + Send + Sync + 'static>;

impl std::fmt::Debug for Asks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Asks")
            .field("holding", &self.holding())
            .finish_non_exhaustive()
    }
}

impl Asks {
    /// A board that tells `tell`, holding for [`HOLD`] and grouping over [`GROUP`].
    pub fn new(tell: Tell) -> Self {
        Self::timed(tell, HOLD, GROUP)
    }

    /// A board that holds for `hold` and groups over `group`.
    pub fn timed(tell: Tell, hold: Duration, group: Duration) -> Self {
        Self {
            state: Mutex::default(),
            changed: Condvar::new(),
            tell,
            hold,
            group,
            known: None,
        }
    }

    /// This board, letting a connection on at once, never held or asked about, where `known`
    /// finds its host allowed already: a person's yes given since the chat started (#1666).
    #[must_use]
    pub fn knowing(self, known: Known) -> Self {
        Self {
            known: Some(known),
            ..self
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many connections it holds now.
    pub fn holding(&self) -> usize {
        self.state().holding
    }

    /// Whether the person is being asked about `host`: a connection to it is held, or gave up
    /// and its ask is still unanswered.
    pub fn asks_about(&self, host: &Host) -> bool {
        self.state()
            .asked
            .iter()
            .any(|target| Host::parse(target).is_ok_and(|one| host.covers(&one)))
    }

    /// The scope a host allowed live lets `host` on `port` through at, where a host listed
    /// without a port is carried on `defaults`.
    pub fn allows(&self, host: &str, port: u16, defaults: &[u16]) -> Option<By> {
        allowed_in(&self.state(), host, port, defaults)
    }

    /// **Holds a connection to `host` on `port`** until the person answers or [`HOLD`] passes:
    /// what the proxy asks of a connection nothing lists. A host listed without a port is
    /// carried on `defaults`. Answered at once where a live Allow or Keep blocked already
    /// covers it, or where the board is full.
    pub fn hold(&self, host: &str, port: u16, defaults: &[u16]) -> Answer {
        let named = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
        let target = host_and_port(&named, port);
        let parsed = Host::parse(&target).ok();
        // Allowed already, since the chat started: on at once, and carried from now on.
        if let (Some(known), Some(host)) = (&self.known, &parsed)
            && let Some(by) = known(host)
        {
            self.allow(host, by);
            return Answer::Allowed(by);
        }
        let mut state = self.state();
        if let Some(by) = allowed_in(&state, &named, port, defaults) {
            return Answer::Allowed(by);
        }
        if kept_blocked(&state, parsed.as_ref()) {
            return Answer::Refused;
        }
        if state.holding >= MOST_HELD {
            return Answer::Busy;
        }
        state.holding += 1;
        let deadline = Instant::now() + self.hold;
        // Asked once: a host already asked about, and not answered, waits on that ask.
        if !state.asked.contains(&target) {
            state.asked.push(target.clone());
            keep_within(&mut state.asked);
            match &mut state.gathering {
                Some((targets, _)) => targets.push(target.clone()),
                None => state.gathering = Some((vec![target.clone()], Instant::now() + self.group)),
            }
        }
        let answer = loop {
            if let Some(by) = allowed_in(&state, &named, port, defaults) {
                break Answer::Allowed(by);
            }
            if kept_blocked(&state, parsed.as_ref()) {
                break Answer::Refused;
            }
            let now = Instant::now();
            // The window has passed: whichever held connection sees it first tells the ask,
            // once, outside the lock.
            if state.gathering.as_ref().is_some_and(|(_, at)| now >= *at) {
                let targets = state
                    .gathering
                    .take()
                    .map(|(targets, _)| targets)
                    .unwrap_or_default();
                drop(state);
                (self.tell)(Heard::Asked(targets));
                state = self.state();
                continue;
            }
            if now >= deadline {
                break Answer::TimedOut;
            }
            let until = state
                .gathering
                .as_ref()
                .map_or(deadline, |(_, at)| (*at).min(deadline));
            state = self
                .changed
                .wait_timeout(state, until.saturating_duration_since(now))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        };
        state.holding -= 1;
        if state.holding == 0 {
            // Nothing is waiting on an ask not told yet: there is nobody to ask for.
            state.gathering = None;
        }
        // Answered or given up: the next connection to it is asked about again, so one whose
        // Notice was put away is not held a minute for nothing.
        state.asked.retain(|one| *one != target);
        if answer == Answer::TimedOut && !state.gave_up.contains(&target) {
            state.gave_up.push(target.clone());
            keep_within(&mut state.gave_up);
        }
        drop(state);
        self.changed.notify_all();
        if answer == Answer::TimedOut {
            (self.tell)(Heard::TimedOut(target));
        }
        answer
    }

    /// **The person allowed `host` at scope `by`**, from the window: every held connection it
    /// covers goes on now, and every later one is carried. Answers the hosts and ports that were
    /// held and gave up before this Allow, once each, which the chat is told to retry.
    pub fn allow(&self, host: &Host, by: By) -> Vec<String> {
        let mut state = self.state();
        let entry = host.to_string();
        if !state
            .live
            .iter()
            .any(|(one, kept)| *one == entry && *kept == by)
        {
            state.live.push((entry, by));
            if state.live.len() > MOST_KEPT {
                state.live.remove(0);
            }
        }
        state.blocked.retain(|one| !host.covers(one));
        let covered = |target: &String| Host::parse(target).is_ok_and(|one| host.covers(&one));
        let retry: Vec<String> = state
            .gave_up
            .iter()
            .filter(|t| covered(t))
            .cloned()
            .collect();
        state.gave_up.retain(|target| !covered(target));
        state.asked.retain(|target| !covered(target));
        drop(state);
        self.changed.notify_all();
        retry
    }

    /// **The person kept `host` blocked**, from the window: what is held on it is refused now,
    /// and so is every later connection to it, until an Allow.
    pub fn keep_blocked(&self, host: &Host) {
        let mut state = self.state();
        if !state.blocked.contains(host) {
            state.blocked.push(host.clone());
            if state.blocked.len() > MOST_KEPT {
                state.blocked.remove(0);
            }
        }
        let other = |target: &String| Host::parse(target).is_ok_and(|one| !host.covers(&one));
        state.gave_up.retain(other);
        state.asked.retain(other);
        drop(state);
        self.changed.notify_all();
    }

    /// **An Allow of `host` at scope `by` was removed**: what it allowed live reaches nothing
    /// again, unless the same host is allowed live at another scope.
    pub fn forget(&self, host: &Host, by: By) {
        let entry = host.to_string();
        self.state()
            .live
            .retain(|(one, kept)| *one != entry || *kept != by);
    }
}

/// The scope a live Allow in `state` carries `host` on `port` at.
fn allowed_in(state: &State, host: &str, port: u16, defaults: &[u16]) -> Option<By> {
    state
        .live
        .iter()
        .find(|(entry, _)| allows_on(std::slice::from_ref(entry), host, port, defaults))
        .map(|(_, by)| *by)
}

/// Whether `target` is one kept blocked in `state`.
fn kept_blocked(state: &State, target: Option<&Host>) -> bool {
    target.is_some_and(|target| state.blocked.iter().any(|one| one.covers(target)))
}

/// Lets the oldest go past [`MOST_KEPT`].
fn keep_within(kept: &mut Vec<String>) {
    if kept.len() > MOST_KEPT {
        let over = kept.len() - MOST_KEPT;
        kept.drain(..over);
    }
}
