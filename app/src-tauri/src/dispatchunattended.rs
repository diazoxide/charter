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
//! **One seam refuses plainly.** An unattended chat's ask goes through
//! [`crate::dispatchgrants::request_dispatch_grant_or_refuse`], which answers it with
//! [`unattended`] and nothing else, so a chat nobody is at is refused in one place.
//!
//! **Outside a sandbox it dispatches to no other persona.** Whether the chat is sandboxed is
//! this app's record of how it started it ([`crate::chats::Chats::confines_of`]): what its
//! sandbox was compiled to, or nothing. No line a chat sends says so.

use std::path::Path;

use purlis_core::dispatchgrant::{self, InForce};
use purlis_core::dispatchunattended::{self, Answer, Attendance};
use purlis_core::sandbox::policy::Locks;

use crate::dispatchgrants::{
    Asking, Requested, request_dispatch_grant, request_dispatch_grant_or_refuse,
};

/// **THE DISPATCH CORE'S ENTRY POINT (#1446).** Chat `session` of `held`'s project, which runs
/// as `attendance` says, asks to dispatch to persona `target` with `brief`, for a task that
/// works in `works_in` (#1505: the workspace the new chat is to run in, none for the project's
/// root).
///
/// An attended chat's ask is [`request_dispatch_grant`]'s, whole. An unattended chat's is
/// answered here and now: [`Requested::Covered`], [`Requested::Locked`] or
/// [`Requested::Refused`], and never [`Requested::NeedsGrant`]. Its brief is not kept: nothing
/// is shown to anyone.
pub fn request_dispatch(
    held: &crate::planes::Held,
    session: u32,
    attendance: Attendance,
    target: &str,
    brief: &str,
    works_in: Option<&str>,
) -> Requested {
    match attendance {
        Attendance::Attended => request_dispatch_grant(held, session, target, brief, works_in),
        // One seam refuses plainly, for whoever calls it: it ends in [`unattended`].
        Attendance::Unattended => {
            request_dispatch_grant_or_refuse(held, session, target, brief, works_in)
        }
    }
}

/// **Whether a person is at chat `session`** (#1501): not where its harness ever reported its
/// prompts off or it was started with them off, and not for a chat this app does not have
/// open. What the window asks before it explains anything on that chat's tab: a chat nobody is
/// at is told nothing, as it is told of no dispatch. It only reads.
#[tauri::command]
#[specta::specta]
pub async fn chat_attended(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    session: u32,
) -> Result<bool, String> {
    let held = planes.held(&plane)?;
    // The profiles are read from the project's files: off the thread the window waits on.
    tauri::async_runtime::spawn_blocking(move || crate::handoff::attended(&held, session))
        .await
        .map_err(|err| format!("asking who is at the chat did not finish: {err}"))
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
/// Of standing grants alone: the person's on this machine and the project's. It makes and
/// widens nothing, and reading the grants in force writes nothing: the project's count only
/// while the last settling of this machine's acceptances answered
/// ([`purlis_core::dispatcharrival::for_read`]).
pub fn unattended(
    root: &Path,
    locks: &Locks,
    asking: &Asking,
    runs: Runs,
    target: &str,
    works_in: Option<&str>,
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
    // For the workspace the task is to work in (#1505): a standing grant limited to one
    // workspace counts for a task there, and for no other.
    let standing = InForce::read(root, Vec::new()).for_task_in(works_in);
    let persona = asking.persona.as_deref();
    // A pair the project's file names counts on this machine only once someone here allowed
    // it (D-1437-R1), so what is in force no longer says whether the file names it. The file
    // is asked: a pair it names that still needs a grant is one nobody here has reviewed.
    let named_by_the_project = persona.is_some_and(|persona| {
        dispatchgrant::committed_at(root)
            .iter()
            .any(|pair| pair.asking == persona && pair.target == target)
    });
    let answer = dispatchunattended::answer_of(
        dispatchgrant::covers(persona, target, &standing, locks),
        persona,
        target,
        named_by_the_project,
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
