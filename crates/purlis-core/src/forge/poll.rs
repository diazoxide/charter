//! When a forge read is repeated (FI7, FI14, FW-4): conditional polling, paced to the
//! account's request budget and to how much a person can see.
//!
//! A [`Poller`] holds the reads one account repeats, its *watches*, and says which are due.
//! Each is sent as a background call through the account's resolver, so it is a conditional
//! `GET` on the native transport ([`super::etag`]) and admitted and counted by the account's
//! meter ([`super::budget`]). Nothing here sends anything or keeps a timer: the caller asks
//! [`Poller::due`] when it wakes, and [`Poller::next_due`] says when to wake.
//!
//! # How often
//!
//! [`interval`] decides it from four things:
//!
//! - **focus**: an open Work view is read every [`WORK_VIEW`], a visible workspace every
//!   [`VISIBLE`], anything else every [`BACKGROUND`], and **nothing at all while every window
//!   is hidden** ([`Focus::Hidden`]);
//! - **the budget**: however many watches there are, they are spread so that polling sends at
//!   most [`PACED`] requests an hour, nine tenths of the allowance, keeping the rest for what a
//!   person asks for. The pacing counts every request sent, a `304` included: GitLab counts
//!   one, and GitHub's secondary limits count requests whatever their answer;
//! - **the forge's limit**: below [`super::budget::FLOOR_PERCENT`] of it remaining, every
//!   interval is [`BACK_OFF`] times longer;
//! - **a spent hour**: nothing is due until it turns, since the meter would hold it back.

use std::time::Duration;

use super::budget::{HOURLY_ALLOWANCE, Usage, WINDOW};
use super::transport::Call;

/// An open Work view's interval.
pub const WORK_VIEW: Duration = Duration::from_secs(60);
/// A visible workspace's interval.
pub const VISIBLE: Duration = Duration::from_secs(5 * 60);
/// Everything else's, while a window is shown.
pub const BACKGROUND: Duration = Duration::from_secs(15 * 60);
/// How many times longer every interval is below the forge's floor.
pub const BACK_OFF: u32 = 4;
/// The requests an hour polling paces itself to, per account.
pub const PACED: u32 = HOURLY_ALLOWANCE / 10 * 9;

/// How much of an account's work a person can see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// A Work view is open on it.
    WorkView,
    /// A workspace with it is visible.
    Visible,
    /// A window is shown, but not on it.
    Background,
    /// Every window is hidden.
    Hidden,
}

impl Focus {
    /// The interval this focus asks for before pacing, or `None` for no polling at all.
    pub fn base(self) -> Option<Duration> {
        match self {
            Focus::WorkView => Some(WORK_VIEW),
            Focus::Visible => Some(VISIBLE),
            Focus::Background => Some(BACKGROUND),
            Focus::Hidden => None,
        }
    }
}

/// How long one watch waits between reads, for `watches` watches on one account at `focus`,
/// given its hour `usage` as of `now`. `None`: do not poll.
pub fn interval(focus: Focus, watches: usize, usage: &Usage, now: u64) -> Option<Duration> {
    let base = focus.base()?;
    let paced = Duration::from_secs((watches as u64 * WINDOW).div_ceil(u64::from(PACED)));
    let mut every = base.max(paced);
    if usage.below_floor(now) {
        every *= BACK_OFF;
    }
    if usage.spent() {
        every = every.max(Duration::from_secs(usage.turns_at().saturating_sub(now)));
    }
    Some(every)
}

/// One repeated read.
#[derive(Debug, Clone)]
struct Watch {
    call: Call,
    /// When it was last read, or `None` before its first.
    last: Option<u64>,
}

/// The reads one account repeats, and when each is due.
#[derive(Debug, Clone, Default)]
pub struct Poller {
    watches: Vec<Watch>,
    /// When the first `due` was asked: the first reads are spread over one interval from it.
    started: Option<u64>,
}

impl Poller {
    pub fn new(calls: Vec<Call>) -> Poller {
        Poller {
            watches: calls
                .into_iter()
                .map(|call| Watch { call, last: None })
                .collect(),
            started: None,
        }
    }

    /// How many watches there are.
    pub fn len(&self) -> usize {
        self.watches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.watches.is_empty()
    }

    /// When watch `i` is next due, at `every`.
    fn due_at(&self, i: usize, every: u64) -> u64 {
        match self.watches[i].last {
            Some(last) => last + every,
            // The first reads are spread over the first interval, not sent at once.
            None => self.started.unwrap_or_default() + every * i as u64 / self.len() as u64,
        }
    }

    /// The reads due at `now` at interval `every`, each marked read. `None` reads nothing.
    /// A changed interval applies at once: a watch is due `every` after its last read.
    pub fn due(&mut self, now: u64, every: Option<Duration>) -> Vec<Call> {
        let Some(every) = every else {
            return Vec::new();
        };
        let every = every.as_secs().max(1);
        self.started.get_or_insert(now);
        let mut due = Vec::new();
        for i in 0..self.watches.len() {
            if self.due_at(i, every) <= now {
                self.watches[i].last = Some(now);
                due.push(self.watches[i].call.clone());
            }
        }
        due
    }

    /// When the next read is due at interval `every`, or `None` when nothing will be.
    pub fn next_due(&self, now: u64, every: Option<Duration>) -> Option<u64> {
        let every = every?.as_secs().max(1);
        if self.started.is_none() {
            return (!self.is_empty()).then_some(now);
        }
        (0..self.watches.len())
            .map(|i| self.due_at(i, every).max(now))
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::budget::ForgeLimit;

    fn idle() -> Usage {
        Usage::default()
    }

    fn calls(n: usize) -> Vec<Call> {
        (0..n)
            .map(|i| Call::get(format!("repos/o/r/pulls/{i}"), crate::forge::LIST_TIMEOUT))
            .collect()
    }

    #[test]
    fn each_focus_has_its_own_interval_and_a_hidden_window_polls_nothing() {
        let at = |focus| interval(focus, 1, &idle(), 0);
        assert_eq!(at(Focus::WorkView), Some(Duration::from_secs(60)));
        assert_eq!(at(Focus::Visible), Some(Duration::from_secs(300)));
        assert_eq!(at(Focus::Background), Some(Duration::from_secs(900)));
        assert_eq!(at(Focus::Hidden), None);
        let mut poller = Poller::new(calls(3));
        assert!(poller.due(0, at(Focus::Hidden)).is_empty());
        assert_eq!(poller.next_due(0, at(Focus::Hidden)), None);
    }

    #[test]
    fn many_watches_are_spread_to_fit_the_budget() {
        // 100 watches at a minute each would be 6,000 an hour; paced to 900, each waits 400 s.
        assert_eq!(
            interval(Focus::WorkView, 100, &idle(), 0),
            Some(Duration::from_secs(400))
        );
    }

    #[test]
    fn below_the_forges_floor_polling_backs_off() {
        let mut usage = idle();
        usage.forge.insert(
            "core".into(),
            ForgeLimit {
                limit: 5000,
                remaining: 999,
                reset: None,
            },
        );
        assert_eq!(
            interval(Focus::WorkView, 1, &usage, 0),
            Some(Duration::from_secs(240))
        );
    }

    #[test]
    fn a_spent_hour_waits_for_it_to_turn() {
        let usage = Usage {
            since: 1_000,
            counted: HOURLY_ALLOWANCE,
            ..Usage::default()
        };
        assert_eq!(
            interval(Focus::WorkView, 1, &usage, 2_000),
            Some(Duration::from_secs(2_600))
        );
    }

    #[test]
    fn first_reads_are_spread_over_one_interval_and_then_repeat_at_it() {
        let every = Some(Duration::from_secs(60));
        let mut poller = Poller::new(calls(4));
        assert_eq!(poller.due(0, every).len(), 1);
        assert_eq!(poller.next_due(0, every), Some(15));
        assert_eq!(poller.due(15, every).len(), 1);
        assert_eq!(poller.due(59, every).len(), 2);
        assert!(poller.due(59, every).is_empty());
        assert_eq!(poller.due(60, every).len(), 1, "the first, a minute on");
    }

    #[test]
    fn a_shorter_interval_applies_at_once() {
        let mut poller = Poller::new(calls(1));
        assert_eq!(poller.due(0, Some(BACKGROUND)).len(), 1);
        assert!(poller.due(30, Some(BACKGROUND)).is_empty());
        assert_eq!(poller.due(60, Some(WORK_VIEW)).len(), 1);
    }
}
