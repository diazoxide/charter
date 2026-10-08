//! **A dispatch refused while nobody was there reaches the person afterwards** (#1507, spec
//! #1483; decision V100-29): the app's half of [`purlis_core::dispatchaway`].
//!
//! # What happens, and when
//!
//! A chat nobody is at is answered from standing grants alone, as before
//! ([`crate::dispatchunattended`]). Where it is refused **only for lack of one** between its
//! persona and another, [`refused`] keeps that it happened, tells the window, and adds one
//! clause to the chat's sentence: the person will see it. Nothing else changes at that
//! moment. No question is put to the chat or to anyone, no dispatch is held, and the chat may
//! do exactly what it could before.
//!
//! The title bar's needs-you list then holds one item a pair and workspace: "<persona> wanted
//! <persona> while you were away", how often and when, with two answers.
//!
//! - **Allow from now on** ([`allow`]) makes the ordinary standing grant for the person on
//!   this machine for that one pair, audited as the person's before it is kept, and takes the
//!   item away. It starts nothing: the dispatch that was refused is gone, and the next run is
//!   what the grant is for.
//! - **Dismiss** ([`dismiss`]) takes the item away and grants nothing.
//!
//! # It is an item of its own
//!
//! It is attached to no chat and no tab: the asking chat is not waiting on the person, so it
//! is not in the project's queue of chats asking, its tab shows no hand, and the counts on the
//! project and workspace tabs do not move. The window lists it from [`dispatch_away`] beside
//! the chats.
//!
//! # A one-press grant, offered on a refused chat's word
//!
//! So the offer is held to the narrowest thing that makes the next run work, and to the
//! app's own facts:
//!
//! - **One pair, for the person, on this machine.** [`allow`] has no level to choose and no
//!   wildcard to name: it writes one named pair to this machine's own record. Nothing here
//!   writes the project's file or "any persona". A grant of this kind covers the pair in every
//!   workspace of the project, which is what the item and its answer say ([`allows_said`]).
//! - **Only a pair that is listed.** [`allow`] refuses a pair with no entry, so the command
//!   grants nothing the person was not shown.
//! - **Checked again at the press**: both names are personas of the project, no policy locks
//!   the pair, and the person has not said never to it. A never said in the meantime drops
//!   the item without a word ([`listed`]), and an Allow that crosses one is refused.
//! - **The item's words are the app's.** The two persona names are the app's record of the
//!   asking chat and one of the project's personas. The task's name is the one text a chat
//!   chose: held to a task name's rule before it is kept, kept short, drawn as text. The brief
//!   is never kept.
//! - **Bounded.** One entry a pair and workspace with a count, and a cap on entries
//!   ([`purlis_core::dispatchaway::MOST`]), so a chat asking in a loop raises a number.
//!
//! **Where the sandbox is what holds that**, as for every dispatch grant (D-1437-R2): a chat
//! that runs without the sandbox runs as the person and can write the record this list is
//! read from, as it can write the grant itself.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use purlis_core::dispatchaway::{self as away, Refused};
use purlis_core::dispatchgrant::{self, Covers, InForce, Pair};
use purlis_core::dispatchunattended::Attendance;
use purlis_core::sandbox;
use purlis_core::sandbox::grant::Level;

use crate::dispatchgrants::{Asking, Audit};
use crate::planes::{PlaneId, Planes};

/// The event the window is sent when a project's list changed: its payload is an
/// [`AwayRefusals`].
pub const CHANGED: &str = "dispatch-away-changed";

/// What a [`sandbox::local::Made`] record calls a dispatch grant, as the grant store does.
const WHAT: &str = "dispatch";

/// One pair refused while nobody was there, as the needs-you list draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AwayRefusal {
    /// The persona the asking chat ran with.
    pub asking: String,
    /// The persona it asked for.
    pub target: String,
    /// The workspace the asking chat worked in; null for the project's root.
    pub workspace: Option<String>,
    /// The name of the task last refused, where it had one.
    pub task: Option<String>,
    /// When it was last refused, in seconds since 1970.
    pub latest: u32,
    /// How many times it was refused.
    pub times: u32,
    /// Exactly what **Allow from now on** allows, and for whom.
    pub allows: String,
}

/// A project's whole list, as the window is told it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AwayRefusals {
    pub plane: PlaneId,
    pub refused: Vec<AwayRefusal>,
}

/// What **Allow from now on** answered: the sentence the window says, and the list as it is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AwayAllowed {
    pub said: String,
    pub refused: Vec<AwayRefusal>,
}

/// Told a project's list each time it changes, for the window.
pub type Teller = Arc<dyn Fn(AwayRefusals) + Send + Sync + 'static>;

static TELL: OnceLock<Teller> = OnceLock::new();

/// Sends every change of a project's list to `tell` from now on: the window, once, as the app
/// starts.
pub fn telling(tell: Teller) {
    let _ = TELL.set(tell);
}

/// What the list is read and answered under: the project, its policy, which names are its
/// personas, the audit and the time.
pub struct On<'a> {
    pub root: &'a Path,
    pub locks: &'a sandbox::policy::Locks,
    /// Whether `name` is a persona of the project now.
    pub known: &'a dyn Fn(&str) -> bool,
    /// Writes the audit of a grant, and answers whether it was written.
    pub audit: Audit<'a>,
    /// Now, in seconds since 1970.
    pub at: u64,
}

/// **Exactly what Allow from now on allows** for `asking` to `target`: said on the item
/// before the press. One pair, for the person, on this machine, in every workspace of the
/// project, and where it is taken back.
pub fn allows_said(asking: &str, target: &str) -> String {
    let (asking, target) = (
        purlis_core::shown::short(asking),
        purlis_core::shown::short(target),
    );
    format!(
        "Allow from now on lets {asking} chats dispatch to {target} without asking, for you on \
         this machine, in every workspace of this project. It allows no other persona and \
         starts nothing now. Revoke it in {}.",
        dispatchgrant::SETTINGS
    )
}

/// What the window says once the grant is kept.
fn allowed_said(asking: &str, target: &str) -> String {
    let (asking, target) = (
        purlis_core::shown::short(asking),
        purlis_core::shown::short(target),
    );
    format!(
        "Allowed for me on this machine: {asking} chats dispatch to {target} without asking \
         from now on, in every workspace of this project. Nothing was started. Revoke it in \
         {}.",
        dispatchgrant::SETTINGS
    )
}

/// What stands for a pair when the person looks.
enum Stands {
    /// Nothing covers it and nothing refuses it: an Allow would mend it.
    Open,
    /// The person said never to it, a policy locks it, a grant covers it by now, or a name is
    /// no longer a persona's: there is nothing left to offer.
    Settled,
}

fn stands(on: &On<'_>, standing: &InForce, asking: &str, target: &str) -> Stands {
    if !(on.known)(asking) || !(on.known)(target) {
        return Stands::Settled;
    }
    match dispatchgrant::covers(Some(asking), target, standing, on.locks) {
        Covers::NeedsGrant => Stands::Open,
        _ => Stands::Settled,
    }
}

fn drawn(entry: Refused) -> AwayRefusal {
    AwayRefusal {
        allows: allows_said(&entry.asking, &entry.target),
        asking: entry.asking,
        target: entry.target,
        workspace: entry.workspace,
        task: entry.task,
        latest: u32::try_from(entry.latest).unwrap_or(u32::MAX),
        times: entry.times,
    }
}

/// **The list as the person is shown it**, newest first: every entry kept, less what is
/// settled by now, which is taken out of the record without a word (a pair the person said
/// never to, one a policy locks, one a grant covers, one whose persona is gone).
///
/// **While this machine's record of nevers does not read, nothing is offered** and nothing is
/// taken out: no grant counts then, and an Allow could be for a pair the person refused.
pub fn listed(on: &On<'_>) -> Vec<AwayRefusal> {
    // No grant of one chat is read: only what stands for the person and the project.
    let standing = InForce::read(on.root, Vec::new());
    if standing.never_unread {
        return Vec::new();
    }
    let mut shown = Vec::new();
    for entry in away::list(on.root, on.at) {
        match stands(on, &standing, &entry.asking, &entry.target) {
            Stands::Open => shown.push(drawn(entry)),
            Stands::Settled => {
                if let Err(why) = away::forget_pair(on.root, &entry.asking, &entry.target, on.at) {
                    tracing::warn!(
                        "purlis: a refusal that is settled could not be taken off the list \
                         ({why})"
                    );
                }
            }
        }
    }
    shown
}

/// **Allow from now on**: lets chats running as `asking` dispatch to `target`, for the person
/// on this machine. Only for a pair the list holds in `workspace`, checked again against the
/// personas, the policy and the nevers as they stand now, audited as the person's before it
/// is kept, and then every entry for the pair is taken away. Starts nothing.
pub fn allow(
    on: &On<'_>,
    asking: &str,
    target: &str,
    workspace: Option<&str>,
) -> Result<String, String> {
    let gone = || {
        "That refusal is no longer listed, so nothing was allowed. Allow the pair from a chat \
         you are at."
            .to_owned()
    };
    let there = away::list(on.root, on.at).into_iter().any(|one| {
        one.asking == asking && one.target == target && one.workspace.as_deref() == workspace
    });
    if !there {
        return Err(gone());
    }
    // Two different personas' names: a wildcard is not one, so none can be granted here.
    let pair = Pair::new(asking, target)?;
    for name in [asking, target] {
        if !(on.known)(name) {
            forget(on, asking, target);
            return Err(format!(
                "This project has no persona named {}, so nothing was allowed.",
                purlis_core::shown::short(name)
            ));
        }
    }
    if let Some(why) = on.locks.dispatch_refused(Some(asking), target) {
        forget(on, asking, target);
        return Err(why);
    }
    // A record of nevers that does not read is not granted across: what the person refused is
    // unknown, and nothing is audited for a grant that would cover nothing.
    if let Some(unread) = dispatchgrant::nevers_unread(on.root) {
        return Err(unread);
    }
    if InForce::read(on.root, Vec::new()).refuses(Some(asking), target) {
        forget(on, asking, target);
        return Err(format!(
            "You said never to {} chats dispatching to {} on this machine, so nothing was \
             allowed. Lift it in {} first.",
            purlis_core::shown::short(asking),
            purlis_core::shown::short(target),
            dispatchgrant::SETTINGS
        ));
    }
    let audited = dispatchgrant::Audited {
        act: dispatchgrant::Act::Grant,
        asking: Some(asking),
        target,
        level: Level::You,
    };
    // The person's, from the window, under no chat: the chat that asked is not who allowed it.
    (on.audit)(None, &audited)?;
    sandbox::local::grant_dispatch(on.root, &pair.asking, &pair.target).map_err(|why| {
        // Recorded as taken back, so the log never ends on a grant that is not there.
        if let Err(unsaid) = (on.audit)(
            None,
            &dispatchgrant::Audited {
                act: dispatchgrant::Act::Revoke,
                ..audited
            },
        ) {
            tracing::warn!(
                "purlis: a dispatch grant that was not kept is still recorded as made ({unsaid})"
            );
        }
        format!("purlis could not keep the grant: {why}")
    })?;
    if let Err(why) = sandbox::local::record_made(
        on.root,
        sandbox::local::Made {
            what: WHAT.to_owned(),
            target: pair.to_string(),
            level: Level::You.word().to_owned(),
            at: on.at,
            chat: None,
        },
    ) {
        // The grant stands, and is audited; only the list's "when" is lost.
        tracing::warn!("purlis: a dispatch grant was kept without when it was made ({why})");
    }
    forget(on, asking, target);
    Ok(allowed_said(asking, target))
}

/// Takes every entry for the pair away; a record that cannot be written is said in the log
/// and costs the answer nothing.
fn forget(on: &On<'_>, asking: &str, target: &str) {
    if let Err(why) = away::forget_pair(on.root, asking, target, on.at) {
        tracing::warn!("purlis: a refusal could not be taken off the list ({why})");
    }
}

/// **Dismiss**: takes the entry for `asking` to `target` in `workspace` away, and grants
/// nothing.
pub fn dismiss(
    on: &On<'_>,
    asking: &str,
    target: &str,
    workspace: Option<&str>,
) -> Result<(), String> {
    away::dismiss(on.root, asking, target, workspace, on.at)
        .map(|_| ())
        .map_err(|why| format!("purlis could not take it off the list: {why}"))
}

/// **Keeps a refusal where it is one a person's grant would mend**, in the project at `root`
/// at `at`, and answers whether the person will see it. `refusal` is what the chat running as
/// `asking` was refused for `target` with, and `said` the sentence it was refused in: kept
/// only where the two are the same refusal, so nothing but the lack of a grant is ever
/// listed.
pub fn keep(
    root: &Path,
    refusal: Option<&purlis_core::dispatchunattended::Refusal>,
    said: &str,
    workspace: Option<&str>,
    task: Option<&str>,
    at: u64,
) -> bool {
    let Some(refusal) = refusal.filter(|refusal| refusal.say() == said) else {
        return false;
    };
    let Some((asking, target)) = away::pair_kept(refusal) else {
        return false;
    };
    match away::keep(root, asking, target, workspace, task, at) {
        Ok(kept) => kept.listed(),
        Err(why) => {
            tracing::warn!("purlis: a dispatch refused with nobody there was not kept ({why})");
            false
        }
    }
}

/// **THE DISPATCH CORE'S SEAM (#1507).** Chat `asking` of `held`'s project, which runs as
/// `attended` says, was refused a dispatch to `target` in the sentence `said`. Answers the
/// sentence the chat is told.
///
/// For a chat a person is at, `said` as it is. For a chat nobody is at, `said` as it is too,
/// unless the refusal was only for lack of a grant between two personas: then it is kept
/// ([`keep`]), the window is told, and the chat reads one more clause, that the person will
/// see it. `workspace` and `task` are the app's own: where its record has the asking chat
/// working, and the task's name as it held it to a task name's rule.
pub fn refused(
    held: &crate::planes::Held,
    attended: Attendance,
    asking: &Asking,
    target: &str,
    said: String,
    workspace: Option<&str>,
    task: Option<&str>,
) -> String {
    if attended != Attendance::Unattended {
        return said;
    }
    let root = held.root();
    let locks = sandbox::policy::Locks::of(root);
    // Read again as the refusal was read, to know which refusal it was: the answer the chat
    // got is a sentence. Nothing is decided by this read.
    let refusal = crate::dispatchunattended::refusal_of(
        root,
        &locks,
        asking,
        crate::dispatchunattended::Runs {
            holds_anothers: asking.held,
            sandboxed: held.chats().confines_of(asking.session).is_some(),
        },
        target,
    );
    if !keep(root, refusal.as_ref(), &said, workspace, task, now_secs()) {
        return said;
    }
    if let Some(tell) = TELL.get() {
        tell(AwayRefusals {
            plane: held.plane_id().clone(),
            refused: with_ground(held, listed),
        });
    }
    away::told(&said)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The ground a window command of `held`'s project stands on.
fn with_ground<T>(held: &crate::planes::Held, with: impl FnOnce(&On<'_>) -> T) -> T {
    let root = held.root();
    let locks = sandbox::policy::Locks::of(root);
    let personas = purlis_core::workspaces::Plane::open(root.to_path_buf())
        .personas()
        .unwrap_or_default();
    with(&On {
        root,
        locks: &locks,
        known: &|name| personas.iter().any(|one| one == name),
        audit: &|number, audited| held.hooks().record_dispatch_grant(root, number, audited),
        at: now_secs(),
    })
}

/// What was refused in this project while nobody was there, for the needs-you list.
#[tauri::command]
#[specta::specta]
pub fn dispatch_away(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<AwayRefusal>, String> {
    let held = planes.held(&plane)?;
    Ok(with_ground(&held, listed))
}

/// **Allow from now on** on a needs-you item: chats running as `asking` may dispatch to
/// `target`, for you on this machine. One named pair the list holds, audited as yours before
/// it is kept; it starts nothing. Answers the sentence to say and the list as it is now.
#[tauri::command]
#[specta::specta]
pub fn allow_dispatch_away(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
    workspace: Option<String>,
) -> Result<AwayAllowed, String> {
    let held = planes.held(&plane)?;
    with_ground(&held, |on| {
        let said = allow(on, &asking, &target, workspace.as_deref())?;
        Ok(AwayAllowed {
            said,
            refused: listed(on),
        })
    })
}

/// **Dismiss** on a needs-you item: the entry is taken away and nothing is granted. Answers
/// the list as it is now.
#[tauri::command]
#[specta::specta]
pub fn dismiss_dispatch_away(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
    workspace: Option<String>,
) -> Result<Vec<AwayRefusal>, String> {
    let held = planes.held(&plane)?;
    with_ground(&held, |on| {
        dismiss(on, &asking, &target, workspace.as_deref())?;
        Ok(listed(on))
    })
}

#[cfg(test)]
#[path = "dispatchaway_tests.rs"]
mod tests;
