//! The open-file limit a process that holds many chats starts with (ADR 0068 §9, SC-15).
//!
//! A host with 200 chats holds several descriptors for each: the terminal's master, its
//! reader's and writer's copies, each view's stream and the hook connections. Fifty chats
//! already held 204 descriptors on Linux (ADR 0082's measurements), and macOS starts an app
//! that launchd started, which is every app opened from the Finder or the Dock, with a soft
//! `RLIMIT_NOFILE` of **256**. So the app and `charterd` raise the soft limit when they start,
//! before anything opens a chat.
//!
//! **To `min(hard, OPEN_MAX)`, and never down.** macOS's hard limit is often unlimited, and
//! asking for an unlimited soft limit there fails with `EINVAL`; its `setrlimit(2)` page says
//! to ask for `OPEN_MAX` instead. The same ceiling is used on Linux, where the hard limit can
//! be systemd's 524 288: every program a chat starts inherits the soft limit, and a program
//! that walks every descriptor number up to it, or hands `select(2)` a number past
//! `FD_SETSIZE`, is slower or wrong for each number past what it needs. 10 240 is ten times
//! what 200 chats hold. A limit the process was given above the ceiling is kept: lowering it
//! would take away what whoever started charter chose to give it.

/// `OPEN_MAX` as macOS's `<sys/syslimits.h>` defines it: the most a soft `RLIMIT_NOFILE` is
/// raised to, on every platform.
pub const OPEN_MAX: u64 = 10_240;

/// What the soft `RLIMIT_NOFILE` should become, given the soft and hard limits the process
/// was started with (`None` is unlimited); `None` when it should be left as it is.
pub fn soft_limit_to_ask(soft: Option<u64>, hard: Option<u64>) -> Option<u64> {
    let wanted = hard.map_or(OPEN_MAX, |hard| hard.min(OPEN_MAX));
    match soft {
        Some(soft) if soft < wanted => Some(wanted),
        _ => None,
    }
}

/// What raising the limit did, for the launch log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raised {
    /// The soft limit was `from`, and is now `to`.
    From { from: u64, to: u64 },
    /// The soft limit was already high enough (`None` is unlimited), and was left alone.
    AlreadyEnough(Option<u64>),
    /// The system refused the new limit; the process goes on with the one it had.
    Refused { from: u64, to: u64 },
}

impl std::fmt::Display for Raised {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::From { from, to } => {
                write!(f, "the open-file limit is raised from {from} to {to}")
            }
            Self::AlreadyEnough(Some(soft)) => write!(f, "the open-file limit is already {soft}"),
            Self::AlreadyEnough(None) => write!(f, "the open-file limit is already unlimited"),
            Self::Refused { from, to } => {
                write!(
                    f,
                    "the open-file limit stays {from}: the system refused {to}"
                )
            }
        }
    }
}

/// Raises this process's soft `RLIMIT_NOFILE` as [`soft_limit_to_ask`] says. Called once, as
/// the app or `charterd` starts. It never fails the start: a refused limit is reported and the
/// process goes on with the one it was given.
#[cfg(unix)]
pub fn raise() -> Raised {
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
    let given = getrlimit(Resource::Nofile);
    let Some(to) = soft_limit_to_ask(given.current, given.maximum) else {
        return Raised::AlreadyEnough(given.current);
    };
    // `soft_limit_to_ask` answers only for a finite soft limit.
    let from = given.current.unwrap_or(to);
    let asked = Rlimit {
        current: Some(to),
        maximum: given.maximum,
    };
    match setrlimit(Resource::Nofile, asked) {
        Ok(()) => Raised::From { from, to },
        Err(_) => Raised::Refused { from, to },
    }
}

/// Windows has no `RLIMIT_NOFILE`.
#[cfg(not(unix))]
pub fn raise() -> Raised {
    Raised::AlreadyEnough(None)
}

#[cfg(test)]
mod tests;
