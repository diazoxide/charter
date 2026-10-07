//! **The app's answer to a dispatch from an unattended chat** (#1446, spec #1434): covered by a
//! grant that already stands, locked by policy, or refused in a sentence. Never held for the
//! person, and never a Notice.
//!
//! # The entry point the dispatch core calls
//!
//! [`request_dispatch`]`(held, session, attendance, target, brief)`, in place of
//! [`crate::dispatchgrants::request_dispatch_grant`], which it hands an attended chat's ask to
//! unchanged. `attendance` is the app's own mark on the chat
//! ([`purlis_core::dispatchunattended::Mark`]), never a word of the request.
//!
//! # What an unattended chat cannot do
//!
//! **Its ask touches nothing a grant is made from.** [`unattended`] reads the grants the person
//! made for themselves on this machine and the project's, and answers. It is handed no store
//! of held dispatches and no way to tell the window, so no dispatch of its is held, no Notice
//! is raised, and there is nothing for an Allow to be pressed on: a grant for this chat cannot
//! be made from it, and one made while a person answered it is not read.
//!
//! **Outside a sandbox it dispatches to no other persona.** Whether the chat is sandboxed is
//! this app's record of how it started it ([`crate::chats::Chats::confines_of`]): what its
//! sandbox was compiled to, or nothing. No line a chat sends says so.

use std::path::Path;

use purlis_core::dispatchgrant::{self, InForce};
use purlis_core::dispatchunattended::{self, Answer, Attendance};
use purlis_core::sandbox::policy::Locks;

use crate::dispatchgrants::{Asking, Requested, asking_of, request_dispatch_grant};

/// **THE DISPATCH CORE'S ENTRY POINT (#1446).** Chat `session` of `held`'s project, which runs
/// as `attendance` says, asks to dispatch to persona `target` with `brief`.
///
/// An attended chat's ask is [`request_dispatch_grant`]'s, whole. An unattended chat's is
/// answered here and now: [`Requested::Covered`], [`Requested::Locked`] or
/// [`Requested::Refused`], and never [`Requested::NeedsGrant`]. Its brief is not kept: nothing
/// is shown to anyone.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the dispatch core calls it (#1436)")
)]
pub fn request_dispatch(
    held: &crate::planes::Held,
    session: u32,
    attendance: Attendance,
    target: &str,
    brief: &str,
) -> Requested {
    if attendance == Attendance::Attended {
        return request_dispatch_grant(held, session, target, brief);
    }
    let root = held.root();
    let Some(asking) = asking_of(held.chats(), root, session) else {
        return Requested::Refused(format!("chat {session} is not one this app has open"));
    };
    let known = purlis_core::workspaces::Plane::open(root.to_path_buf())
        .personas()
        .is_ok_and(|personas| personas.iter().any(|one| one == target));
    if !known {
        return Requested::Refused(format!(
            "this project has no persona named {}, so there is nothing to dispatch to.",
            purlis_core::shown::short(target)
        ));
    }
    let runs = Runs {
        holds_anothers: held
            .chats()
            .recorded_chat(session)
            .is_some_and(|chat| chat.held.is_some()),
        sandboxed: held.chats().confines_of(session).is_some(),
    };
    unattended(root, &Locks::of(root), &asking, runs, target)
}

/// How an asking chat runs, as this app started and records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Runs {
    /// Whether it holds another persona's grants in place of its own (#1362).
    pub holds_anothers: bool,
    /// Whether this app started it inside a sandbox.
    pub sandboxed: bool,
}

/// What a chat that holds another persona's grants is told (#1362): what it runs with is not
/// yet its own persona's, and only the person's Allow on its tab makes it so.
pub const HOLDS_ANOTHERS: &str = "this chat still runs on the grants of the chat that opened \
     it, and it runs with its harness's permission prompts off, so nobody is here to allow it \
     its own. It dispatches to nobody until a person allows them on its tab.";

/// **What an unattended chat, `asking`, is answered when it asks to dispatch to `target`** in
/// the project at `root` under `locks`. `runs` is how this app started and records the chat.
///
/// Read-only, and of standing grants alone: the person's on this machine and the project's.
pub fn unattended(
    root: &Path,
    locks: &Locks,
    asking: &Asking,
    runs: Runs,
    target: &str,
) -> Requested {
    if !purlis_core::personas::valid_name(target) {
        return Requested::Refused(format!(
            "{} is not a persona's name, so there is nothing to dispatch to.",
            purlis_core::shown::short(target)
        ));
    }
    if runs.holds_anothers {
        return Requested::Refused(HOLDS_ANOTHERS.to_owned());
    }
    // No grant of one chat is read, so none can count.
    let standing = InForce::read(root, Vec::new());
    let answer = dispatchunattended::covers(
        asking.persona.as_deref(),
        target,
        &standing,
        locks,
        runs.sandboxed,
    );
    match answer {
        Answer::Covered => Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat(target)),
        Answer::Locked(why) => Requested::Locked(why),
        Answer::Refused(why) => Requested::Refused(why.say()),
    }
}

#[cfg(test)]
#[path = "dispatchunattended_tests.rs"]
mod tests;
