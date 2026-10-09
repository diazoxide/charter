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
//! <persona> while you were away", how often and when, with three answers.
//!
//! - **Dismiss** ([`dismiss`]) puts the item away and grants nothing. It holds: the entry is
//!   kept, marked, and further refusals for it are counted without listing it and without the
//!   clause in the chat's sentence, until one comes a week or more after the dismissal
//!   ([`purlis_core::dispatchaway::QUIET_SECS`]).
//! - **Never for this pair** ([`never`]) is the store's own never, kept for the person on this
//!   machine and audited as theirs: no chat of that persona is asked or allowed for that
//!   target until they lift it in Settings, and nothing more is kept for the pair.
//! - **Allow from now on** ([`allow`]) makes the ordinary standing grant for the person on
//!   this machine for that one pair, audited as theirs before it is kept, and takes the item
//!   away. It starts nothing: the dispatch that was refused is gone, and the next run is what
//!   the grant is for.
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
//! So the offer is held to the app's own facts, and a chat is given nothing to steer it with:
//!
//! - **One pair, for the person, on this machine.** [`allow`] has no level to choose and no
//!   wildcard to name: it writes one named pair to this machine's own record. Nothing here
//!   writes the project's file or "any persona".
//! - **For the workspace the refused task would have worked in, and no other** (#1505): the
//!   entry keeps that workspace, by the app's own record of the dispatch, and the grant is
//!   limited to it ([`purlis_core::dispatchwithin::grant_yours`]). A refusal at the project's
//!   root has no workspace to limit a grant to, so there the grant holds in any workspace
//!   and the item says "in any workspace". No grant is kept for a name that is no workspace
//!   of the project now: nothing is audited, and the sentence says why.
//! - **The sentence before the press says the grant's reach** ([`allows_said`]): where it
//!   holds, that a chat nobody is at uses it too, and what the target works with, in the
//!   words the dispatch question says it (#1502): its vaults, its hosts, and to whom its
//!   own chats may dispatch onward under what stands.
//! - **Only a pair that is listed.** [`allow`] and [`never`] refuse a pair with no listed
//!   entry, so a command grants nothing the person was not shown.
//! - **Checked again at the press**: both names are personas of the project, no policy locks
//!   the pair, and the person has not said never to it. A never said in the meantime drops
//!   the item without a word ([`listed`]), and an Allow that crosses one is refused.
//! - **Nothing a chat wrote is drawn.** The two persona names and the workspace are the app's
//!   record of the asking chat and the project; the count and the times are its own. The
//!   brief and the task's name are never kept.
//! - **A chat cannot move a row.** The list is in the order pairs were first refused, and a
//!   repeat changes a count and nothing else, so asking again puts no row under a pointer.
//! - **A chat cannot bring back what was put away**, and is not told whether a refusal was
//!   listed once the person dismissed it.
//! - **Bounded.** One entry a pair and workspace with a count, and a cap on entries
//!   ([`purlis_core::dispatchaway::MOST`]), so a chat asking in a loop raises a number.
//! - **The window's alone.** The four commands are served on no link
//!   (`purlis_session_protocol::ui::WINDOW_ONLY`), and no line on the hook socket names one.
//! - **Audited with where it came from.** The grant's and the never's record in the event log
//!   says `from: "away"`, so it can be told from one made in Settings.
//!
//! **Where the sandbox is what holds that**, as for every dispatch grant (D-1437-R2): a chat
//! that runs without the sandbox runs as the person and can write the record this list is
//! read from, as it can write the grant itself. Nothing is kept on such a chat's word: its
//! refusal is another one ([`purlis_core::dispatchunattended::Refusal::Unsandboxed`]).
//!
//! # Reading it, and tidying it
//!
//! [`shown`] reads, and writes no more than a look at the grants writes (a workspace's
//! count of times seen gone). [`listed`] is [`shown`] and then takes out of the
//! record what is settled; it is what the window's commands answer with, **and is never
//! called while a dispatch is being decided**. [`refused`] runs under the lock a decision is
//! made under: it writes the one refusal, and the window is told from a thread of its own.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use purlis_core::dispatchaway::{self as away, Refused};
use purlis_core::dispatchgrant::{self, Covers, InForce, Pair};
use purlis_core::dispatchunattended::Attendance;
use purlis_core::dispatchwants::Access;
use purlis_core::dispatchwithin::{self, Within};
use purlis_core::sandbox;
use purlis_core::sandbox::grant::Level;

use crate::dispatchgrants::Asking;
use crate::planes::{PlaneId, Planes};

/// The event the window is sent when a project's list changed: its payload is an
/// [`AwayRefusals`].
pub const CHANGED: &str = "dispatch-away-changed";

/// What a [`sandbox::local::Made`] record calls a dispatch grant, as the grant store does.
const WHAT: &str = "dispatch";

/// One pair refused while nobody was there, as the needs-you list draws it. **Every field is
/// the app's own**: nothing here is a chat's text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AwayRefusal {
    /// The persona the asking chat ran with.
    pub asking: String,
    /// The persona it asked for.
    pub target: String,
    /// The workspace the refused task would have worked in; null for the project's root.
    /// **Allow from now on** is limited to it, and holds in any workspace where it is null.
    pub workspace: Option<String>,
    /// When it was last refused, in seconds since 1970.
    pub latest: u32,
    /// How many times it was refused.
    pub times: u32,
    /// Exactly what **Allow from now on** allows, for whom, and what it makes reachable.
    pub allows: String,
    /// Where the workspace the refused task would have worked in is not one of the project's
    /// now: the sentence saying so. Allow from now on keeps nothing for such an item.
    pub nowhere: Option<String>,
    /// **What the item said, as a digest**: the pair, the level, the workspace, whether it is
    /// there, and what the target works with. Allow from now on sends it back, and one sent
    /// for an item that reads differently now grants nothing ([`CHANGED_AWAY`]).
    pub shown: String,
}

/// What Allow from now on is answered where the item no longer reads as it was shown.
pub const CHANGED_AWAY: &str = "What this item says changed since it was shown, so nothing \
     was allowed. Read it again, then answer.";

/// The digest an item of `asking` to `target` in `workspace` is drawn with, where `nowhere`
/// says whether its workspace is gone and `access` is what the target works with.
fn shown_of(
    asking: &str,
    target: &str,
    workspace: Option<&str>,
    nowhere: Option<&str>,
    access: &Access,
) -> String {
    let reach = purlis_core::dispatchwants::Offer {
        target: access.clone(),
        also: Vec::new(),
    }
    .stamp();
    format!(
        "{asking}\u{1f}{target}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{reach}",
        Level::You.word(),
        workspace.unwrap_or_default(),
        nowhere.unwrap_or_default()
    )
}

/// Where the workspace of an item is not one of the project at `root` now: why.
fn nowhere_of(root: &Path, workspace: Option<&str>) -> Option<String> {
    workspace
        .filter(|name| !dispatchwithin::Seen::read(root).is_there(name))
        .map(dispatchwithin::not_there_said)
}

/// A project's whole list, as the window is told it: in the order pairs were first refused.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AwayRefusals {
    pub plane: PlaneId,
    pub refused: Vec<AwayRefusal>,
}

/// What **Allow from now on** and **Never for this pair** answered: the sentence the window
/// says, and the list as it is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AwayAnswered {
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

/// Writes the audit of a grant or a never made on an item, and answers whether it was
/// written. The record says where it came from
/// ([`crate::hooks::Hooks::record_dispatch_grant_from_away`]).
pub type Audit<'a> = &'a dyn Fn(&dispatchgrant::Audited<'_>) -> Result<(), String>;

/// Whether a name is one of the project's personas now.
pub type Known<'a> = &'a dyn Fn(&str) -> bool;

/// What the list is read and answered under: the project, its policy, which names are its
/// personas, the audit and the time.
pub struct On<'a> {
    pub root: &'a Path,
    pub locks: &'a sandbox::policy::Locks,
    /// Whether `name` is a persona of the project now. **`None` where the project's personas
    /// could not be read**: then nothing is offered, nothing is allowed, and nothing is taken
    /// out of the record.
    pub known: Option<Known<'a>>,
    /// Writes the audit of a grant or a never, and answers whether it was written.
    pub audit: Audit<'a>,
    /// Now, in seconds since 1970.
    pub at: u64,
}

/// Where **Allow from now on** on an item for `workspace` holds, as its sentences end it: the
/// workspace the refused task would have worked in, or any workspace for one at the
/// project's root, which works in none.
fn holds_said(workspace: Option<&str>) -> String {
    match workspace {
        Some(name) => format!("for work in {} only", purlis_core::shown::short(name)),
        None => Within::Any.said(),
    }
}

/// **Exactly what Allow from now on allows** for `asking` to `target`, refused for a task in
/// `workspace`: said on the item before the press. The one pair, for the person, on this
/// machine; **its reach**: where it holds, that a chat nobody is at may use it too, and
/// `works_with`, what the target works with in the dispatch question's own words
/// ([`purlis_core::dispatchwants::Access::said`], #1502), which names to whom its chats may
/// dispatch onward; and where it is taken back.
pub fn allows_said(
    asking: &str,
    target: &str,
    workspace: Option<&str>,
    works_with: &str,
) -> String {
    let (asking, target) = (
        purlis_core::shown::short(asking),
        purlis_core::shown::short(target),
    );
    format!(
        "Allow from now on lets {asking} chats dispatch to {target} without asking, {}: this \
         one pair, for you on this machine. A chat nobody is at may use it too. {works_with} \
         It starts nothing now. Revoke it in {}.",
        holds_said(workspace),
        dispatchgrant::SETTINGS
    )
}

/// What the window says once the grant is kept.
fn allowed_said(asking: &str, target: &str, workspace: Option<&str>) -> String {
    let (asking, target) = (
        purlis_core::shown::short(asking),
        purlis_core::shown::short(target),
    );
    format!(
        "Allowed for me on this machine: {asking} chats dispatch to {target} without asking \
         from now on, {}. Nothing was started. Revoke it in {}.",
        holds_said(workspace),
        dispatchgrant::SETTINGS
    )
}

/// What the window says once the never is kept.
fn never_said(asking: &str, target: &str) -> String {
    let (asking, target) = (
        purlis_core::shown::short(asking),
        purlis_core::shown::short(target),
    );
    format!(
        "No {asking} chat dispatches to {target} on this machine from now on, and nothing more \
         is listed for the pair. Lift it in {}.",
        dispatchgrant::SETTINGS
    )
}

/// What is said where the project's personas could not be read.
const PERSONAS_UNREAD: &str =
    "purlis could not read this project's personas just now, so nothing was changed. Try again.";

/// What [`shown`] read: the list, and the pairs that are settled.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shown {
    /// The list as the person is shown it, in the order pairs were first refused.
    pub listed: Vec<AwayRefusal>,
    /// Entries there is nothing left to offer for, each as asking, target and workspace: the
    /// person said never to the pair, a policy locks it, a grant covers it **for work in
    /// that workspace** by now, or a name is no longer a persona's.
    pub settled: Vec<(String, String, Option<String>)>,
}

/// **The list as it stands**, read and nothing more: every listed entry the record keeps,
/// less what is settled by now, which is named and left where it is.
///
/// **Nothing is offered, and nothing is called settled**, while this machine's record of
/// nevers does not read (no grant counts then, and an Allow could be for a pair the person
/// refused) and where the project's personas could not be read (`on.known` is `None`: a read
/// that failed is not a persona that is gone).
pub fn shown(on: &On<'_>) -> Shown {
    let Some(known) = on.known else {
        return Shown::default();
    };
    // No grant of one chat is read: only what stands for the person and the project.
    let standing = InForce::read(on.root, Vec::new());
    if standing.never_unread {
        return Shown::default();
    }
    let mut shown = Shown::default();
    // What each target works with, read once a target.
    let mut works_with: Vec<(String, Access)> = Vec::new();
    for entry in away::kept(on.root, on.at) {
        // Judged as the refusal was: for a task in the entry's workspace (#1505), so a grant
        // limited to another workspace settles nothing here.
        let there = standing.clone().for_task_in(entry.workspace.as_deref());
        let judged = dispatchgrant::covers(Some(&entry.asking), &entry.target, &there, on.locks);
        // A crossing is settled only by what carries a crossing: a grant that names the pair
        // and covers that workspace (D-1453-16). "Any persona" is what it was refused despite,
        // so it settles nothing here. A never, a lock and a persona gone settle it as any.
        let open = known(&entry.asking)
            && known(&entry.target)
            && if entry.crossing {
                matches!(judged, Covers::Covered | Covers::NeedsGrant)
                    && !matches!(
                        there.named_level_in(
                            Some(&entry.asking),
                            &entry.target,
                            entry.workspace.as_deref()
                        ),
                        Some(Level::You | Level::Project)
                    )
            } else {
                judged == Covers::NeedsGrant
            };
        if !open {
            let one = (entry.asking, entry.target, entry.workspace);
            if !shown.settled.contains(&one) {
                shown.settled.push(one);
            }
        } else if entry.dismissed.is_none() {
            let access = match works_with
                .iter()
                .find(|(target, _)| *target == entry.target)
            {
                Some((_, access)) => access.clone(),
                None => {
                    let access = Access::at(on.root, on.locks, &entry.target);
                    works_with.push((entry.target.clone(), access.clone()));
                    access
                }
            };
            let nowhere = nowhere_of(on.root, entry.workspace.as_deref());
            shown.listed.push(drawn(entry, &access, nowhere));
        }
    }
    shown
}

fn drawn(entry: Refused, access: &Access, nowhere: Option<String>) -> AwayRefusal {
    AwayRefusal {
        shown: shown_of(
            &entry.asking,
            &entry.target,
            entry.workspace.as_deref(),
            nowhere.as_deref(),
            access,
        ),
        // Said before the press, where it is so: Allow keeps nothing for a workspace that is
        // not there, and the item says why rather than what a grant would reach.
        allows: match &nowhere {
            Some(why) => format!(
                "{why} So Allow from now on keeps nothing for this item; put it away with \
                 Dismiss."
            ),
            None => allows_said(
                &entry.asking,
                &entry.target,
                entry.workspace.as_deref(),
                &access.said(),
            ),
        },
        nowhere,
        asking: entry.asking,
        target: entry.target,
        workspace: entry.workspace,
        latest: u32::try_from(entry.latest).unwrap_or(u32::MAX),
        times: entry.times,
    }
}

/// **The list as the person is shown it, with the record tidied**: [`shown`], and then every
/// settled entry is taken out of the record without a word, a dismissed one too.
///
/// **A read that writes.** It is what the window's commands answer with, on the person's own
/// look or press. It is never called while a dispatch is being decided: [`refused`] tells the
/// window from [`shown`], off the lock.
pub fn listed(on: &On<'_>) -> Vec<AwayRefusal> {
    let shown = shown(on);
    for (asking, target, workspace) in &shown.settled {
        forget_in(on, asking, target, workspace.as_deref());
    }
    shown.listed
}

/// Whether the list holds an entry for `asking` to `target` in `workspace` that the person is
/// shown: not one they dismissed.
fn is_listed(on: &On<'_>, asking: &str, target: &str, workspace: Option<&str>) -> bool {
    away::list(on.root, on.at).into_iter().any(|one| {
        one.asking == asking && one.target == target && one.workspace.as_deref() == workspace
    })
}

/// **Allow from now on**: lets chats running as `asking` dispatch to `target`, for the person
/// on this machine, **for work in `workspace`**: the workspace the refused task would have
/// worked in (#1505), or any workspace where that was the project's root. Only for a pair the
/// list holds in `workspace`, checked again against the personas, the policy and the nevers
/// as they stand now, audited as the person's before it is kept. Then the entry goes, and
/// with a grant that holds in any workspace every entry for the pair. Starts nothing.
///
/// **No grant is kept for a name that is no workspace of the project now**: it would belong
/// to whatever is made under that name next. Said before anything is audited
/// ([`dispatchwithin::not_there_said`]); nothing is kept, and the item stays for the person
/// to put away.
///
/// **Held to what the item said** (`shown`, [`AwayRefusal::shown`]): where the item reads
/// differently now (what the target works with changed, or its workspace went), nothing is
/// audited or kept and the answer is [`CHANGED_AWAY`]; the window reads the list again.
/// `None` asks nothing of it: a caller inside the app that showed nothing.
pub fn allow(
    on: &On<'_>,
    asking: &str,
    target: &str,
    workspace: Option<&str>,
    shown: Option<&str>,
) -> Result<String, String> {
    if !is_listed(on, asking, target, workspace) {
        return Err(
            "That refusal is no longer listed, so nothing was allowed. Allow the pair from a \
             chat you are at."
                .to_owned(),
        );
    }
    if let Some(shown) = shown {
        let nowhere = nowhere_of(on.root, workspace);
        let now = shown_of(
            asking,
            target,
            workspace,
            nowhere.as_deref(),
            &Access::at(on.root, on.locks, target),
        );
        if now != shown {
            return Err(CHANGED_AWAY.to_owned());
        }
    }
    // Two different personas' names: a wildcard is not one, so none can be granted here.
    let pair = Pair::new(asking, target)?;
    let Some(known) = on.known else {
        return Err(PERSONAS_UNREAD.to_owned());
    };
    for name in [asking, target] {
        if !known(name) {
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
    // **Where it holds**: the workspace the refused task would have worked in, and any
    // workspace for one at the project's root. Checked before anything is audited: no grant
    // is kept, and none is recorded, for a name that is no workspace now.
    let within = Within::of(workspace);
    if let Some(name) = workspace
        && !dispatchwithin::Seen::read(on.root).is_there(name)
    {
        return Err(dispatchwithin::not_there_said(name));
    }
    let limited = workspace
        .map(|name| dispatchwithin::Limited::of(&pair, name))
        .transpose()?;
    let audited = dispatchgrant::Audited {
        act: dispatchgrant::Act::Grant,
        asking: Some(asking),
        target,
        level: Level::You,
        workspace: within.workspace(),
    };
    // The person's, from the window, under no chat: the chat that asked is not who allowed it.
    (on.audit)(&audited)?;
    dispatchwithin::grant_yours(on.root, &pair, &within).map_err(|why| {
        // Recorded as taken back, so the log never ends on a grant that is not there.
        if let Err(unsaid) = (on.audit)(&dispatchgrant::Audited {
            act: dispatchgrant::Act::Revoke,
            ..audited
        }) {
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
            // As the grant store records one, so Settings' table finds "when".
            target: limited
                .as_ref()
                .map_or_else(|| pair.to_string(), ToString::to_string),
            level: Level::You.word().to_owned(),
            at: on.at,
            chat: None,
        },
    ) {
        // The grant stands, and is audited; only the list's "when" is lost.
        tracing::warn!("purlis: a dispatch grant was kept without when it was made ({why})");
    }
    // What the grant covers is answered: the one entry for a grant limited to its workspace,
    // every entry of the pair for one that holds in any.
    match workspace {
        Some(_) => forget_in(on, asking, target, workspace),
        None => forget(on, asking, target),
    }
    Ok(allowed_said(asking, target, workspace))
}

/// **Never for this pair**, said on an item: the store's own never for `asking` to `target`,
/// kept for the person on this machine and audited as theirs before it is kept. Only for a
/// pair the list holds in `workspace`. Every entry for the pair then goes, and no later
/// refusal is kept for it: a never is not a refusal a grant would mend.
pub fn never(
    on: &On<'_>,
    asking: &str,
    target: &str,
    workspace: Option<&str>,
) -> Result<String, String> {
    if !is_listed(on, asking, target, workspace) {
        return Err(
            "That refusal is no longer listed, so nothing was changed. Say never for the pair \
             from a chat you are at."
                .to_owned(),
        );
    }
    let pair = Pair::new(asking, target)?;
    // A record that does not read is not written over: said before anything is audited.
    if let Some(unread) = dispatchgrant::nevers_unread(on.root) {
        return Err(unread);
    }
    // A never is not limited to a workspace (#1505): it holds in every one.
    let audited = dispatchgrant::Audited {
        act: dispatchgrant::Act::Never,
        asking: Some(asking),
        target,
        level: Level::You,
        workspace: None,
    };
    (on.audit)(&audited)?;
    if let Err(why) = dispatchgrant::never(on.root, &pair) {
        // Recorded as lifted, so the log never ends on a never that is not there.
        if let Err(unsaid) = (on.audit)(&dispatchgrant::Audited {
            act: dispatchgrant::Act::LiftNever,
            ..audited
        }) {
            tracing::warn!(
                "purlis: a never that was not kept is still recorded as made ({unsaid})"
            );
        }
        return Err(format!("purlis could not keep it: {why}"));
    }
    forget(on, asking, target);
    Ok(never_said(asking, target))
}

/// Takes every entry for the pair away; a record that cannot be written is said in the log
/// and costs the answer nothing.
fn forget(on: &On<'_>, asking: &str, target: &str) {
    if let Err(why) = away::forget_pair(on.root, asking, target, on.at) {
        tracing::warn!("purlis: a refusal could not be taken off the list ({why})");
    }
}

/// Takes the pair's one entry for `workspace` away, as [`forget`] takes them all.
fn forget_in(on: &On<'_>, asking: &str, target: &str, workspace: Option<&str>) {
    if let Err(why) = away::forget_in(on.root, asking, target, workspace, on.at) {
        tracing::warn!("purlis: a refusal could not be taken off the list ({why})");
    }
}

/// **Dismiss**: puts the entry for `asking` to `target` in `workspace` away, and grants
/// nothing. It holds: the entry is kept, marked, and is listed again only by a refusal that
/// comes a week or more later ([`purlis_core::dispatchaway::QUIET_SECS`]).
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
/// at `at`, and answers whether the person will see it. `refusal` is what the chat was refused
/// with, and `said` the sentence it was refused in: kept only where the two are the same
/// refusal, so nothing but the lack of a grant is ever listed. **`false` for one the person
/// dismissed**: it is counted, and the chat is told nothing of it.
pub fn keep(
    root: &Path,
    refusal: Option<&purlis_core::dispatchunattended::Refusal>,
    said: &str,
    workspace: Option<&str>,
    at: u64,
) -> bool {
    let Some(refusal) = refusal.filter(|refusal| refusal.say() == said) else {
        return false;
    };
    let Some((asking, target)) = away::pair_kept(refusal) else {
        return false;
    };
    match away::keep(root, asking, target, workspace, at) {
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
/// unless the refusal was only for lack of a grant between two personas and the person has
/// not put it away: then it is kept ([`keep`]), the window is told, and the chat reads one
/// more clause, that the person will see it. `workspace` is the app's own: **where the
/// refused task would have worked** (#1505, [`purlis_core::dispatchwithin::works_in`]), which
/// is the asking chat's own workspace unless the dispatch named or moved into another.
/// **No word of the request is taken**: a workspace it names is looked up among the
/// project's own before it is ever this value.
///
/// **Called while the dispatch is being decided**, so it does as little as keeps the
/// refusal: one read of the answer, one write of the entry. The window is told from a thread
/// of its own, from a read that writes nothing ([`shown`]).
pub fn refused(
    held: &crate::planes::Held,
    attended: Attendance,
    asking: &Asking,
    target: &str,
    said: String,
    workspace: Option<&str>,
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
        workspace,
    );
    if !keep(root, refusal.as_ref(), &said, workspace, now_secs()) {
        return said;
    }
    // Off the lock the decision is made under: a chat asking in a loop costs the other
    // dispatches one write each, and the reading is done on a thread of its own.
    tell_the_window(held);
    away::told(&said)
}

/// **A chat nobody is at was refused a crossing into `workspace`** (D-1453-16, #1505): to start
/// a chat as `target` there it needs a grant that names the pair and covers that workspace,
/// and none stands. Kept like a refusal for lack of a grant ([`refused`]), for the item's Allow
/// mends it: a grant for work in that workspace is what the crossing rule asks for. Only a
/// sandboxed chat's, on its own grants, between two personas. Answers the sentence the chat
/// is told.
pub fn refused_crossing(
    held: &crate::planes::Held,
    asking: &Asking,
    target: Option<&str>,
    said: String,
    workspace: &str,
) -> String {
    let (Some(persona), Some(target)) = (asking.persona.as_deref(), target) else {
        return said;
    };
    if asking.held || held.chats().confines_of(asking.session).is_none() {
        return said;
    }
    match away::keep_crossing(held.root(), persona, target, workspace, now_secs()) {
        Ok(kept) if kept.listed() => {
            tell_the_window(held);
            away::told(&said)
        }
        Ok(_) => said,
        Err(why) => {
            tracing::warn!("purlis: a crossing refused with nobody there was not kept ({why})");
            said
        }
    }
}

/// Tells the window `held`'s list, from a thread of its own and a read that decides nothing.
fn tell_the_window(held: &crate::planes::Held) {
    if let Some(tell) = TELL.get().cloned() {
        let (root, plane) = (held.root().to_path_buf(), held.plane_id().clone());
        let told = std::thread::Builder::new()
            .name("purlis-away-told".to_owned())
            .spawn(move || {
                let refused = reading(&root, |on| shown(on).listed);
                tell(AwayRefusals { plane, refused });
            });
        if let Err(why) = told {
            tracing::warn!("purlis: the window was not told of a refusal that was kept ({why})");
        }
    }
}

/// The list of the project at `root` as the window reads it ([`listed`]): for the tests of
/// other modules, which hold no window.
#[cfg(test)]
pub(crate) fn listed_at(root: &Path) -> Vec<AwayRefusal> {
    reading(root, listed)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Runs `with` on the project at `root` as it stands now, with an audit that records
/// nothing: for a read.
fn reading<T>(root: &Path, with: impl FnOnce(&On<'_>) -> T) -> T {
    let unrecorded = |_: &dispatchgrant::Audited<'_>| {
        Err("this is a read: nothing is changed from it".to_owned())
    };
    grounded(root, &unrecorded, with)
}

fn grounded<T>(root: &Path, audit: Audit<'_>, with: impl FnOnce(&On<'_>) -> T) -> T {
    let locks = sandbox::policy::Locks::of(root);
    // A read that fails is not a project with no personas: it is said as unread.
    let personas = purlis_core::workspaces::Plane::open(root.to_path_buf())
        .personas()
        .ok();
    let known = |name: &str| {
        personas
            .as_ref()
            .is_some_and(|personas| personas.iter().any(|one| one == name))
    };
    with(&On {
        root,
        locks: &locks,
        known: personas.is_some().then_some(&known as Known<'_>),
        audit,
        at: now_secs(),
    })
}

/// The ground a window command of `held`'s project stands on.
fn with_ground<T>(held: &crate::planes::Held, with: impl FnOnce(&On<'_>) -> T) -> T {
    let root = held.root();
    grounded(
        root,
        &|audited| held.hooks().record_dispatch_grant_from_away(root, audited),
        with,
    )
}

/// What was refused in this project while nobody was there, for the needs-you list: in the
/// order pairs were first refused, which no later refusal moves.
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
/// `target`, for you on this machine, for work in `workspace`, the one the refused task would
/// have worked in (null, the project's root: in any workspace). One named pair the list
/// holds for that workspace, audited as yours before it is kept; it starts nothing. `shown`
/// is the item's digest as the window drew it: an item that reads differently now grants
/// nothing. Answers the sentence to say and the list as it is now.
#[tauri::command]
#[specta::specta]
pub fn allow_dispatch_away(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
    workspace: Option<String>,
    shown: String,
) -> Result<AwayAnswered, String> {
    let held = planes.held(&plane)?;
    with_ground(&held, |on| {
        let said = allow(on, &asking, &target, workspace.as_deref(), Some(&shown))?;
        Ok(AwayAnswered {
            said,
            refused: listed(on),
        })
    })
}

/// **Never for this pair** on a needs-you item: no chat running as `asking` is asked or
/// allowed to dispatch to `target` on this machine until you lift it in Settings, and nothing
/// more is listed for the pair. Audited as yours before it is kept. Answers the sentence to
/// say and the list as it is now.
#[tauri::command]
#[specta::specta]
pub fn never_dispatch_away(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
    workspace: Option<String>,
) -> Result<AwayAnswered, String> {
    let held = planes.held(&plane)?;
    with_ground(&held, |on| {
        let said = never(on, &asking, &target, workspace.as_deref())?;
        Ok(AwayAnswered {
            said,
            refused: listed(on),
        })
    })
}

/// **Dismiss** on a needs-you item: it is put away and nothing is granted. Further refusals
/// for it are counted and not listed, until one comes a week or more later. Answers the list
/// as it is now.
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
