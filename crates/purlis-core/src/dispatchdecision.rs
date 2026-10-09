//! Whether a dispatch may start: the one decision every dispatch goes through (#1434, #1436).
//!
//! A dispatch is one chat starting another, running as a persona, with a brief (ADR 0090). An
//! agent's `purlis dispatch`, the matching chat tool, the person's own dispatch from a tab and
//! the vault-refused Notice's button all ask [`decide`], and nothing starts a persona chat
//! without it answering [`Decision::Start`].
//!
//! **Pure, and fed only what the app holds.** Every field of a [`Request`] is the app's own
//! record of the asking chat, the project's definition of the persona, and the limits and
//! grants in force. Nothing in it is a word the asking chat sent, except the persona it names
//! and the profile it asks for, and a profile is only ever looked up among the ones the
//! project offers. So what a chat can say changes which persona is asked for and on which of
//! the project's profiles, and nothing else about the answer.
//!
//! # The parts, and where they are joined
//!
//! Three modules each answer one question, and [`asked_by_a_chat`] is the one place that asks
//! all three for a dispatch:
//!
//! - [`crate::dispatchlimits`]: the limits in force ([`crate::dispatchlimits::of`]) and whether
//!   the asking chat's lineage is within them ([`crate::dispatchlimits::decide`]);
//! - [`crate::dispatchgrant`]: whether a grant or the same persona covers the pair, or a
//!   policy locks it ([`crate::dispatchgrant::covers`]);
//! - [`crate::personaprofile`]: the profile the new chat starts on
//!   ([`crate::personaprofile::for_dispatch`]), which the caller asks, because the answer is
//!   also what the chat is then started from.
//!
//! # The order of the checks
//!
//! 1. the asker is a chat, not a harness's helper sub-agent ([`Refused::Helper`]). Only the
//!    tool hook asks this ([`crate::dispatchguard`]): it alone knows a sub-agent made the call,
//!    and the app, which cannot tell a sub-agent's ask from its chat's, never builds one;
//! 2. the asking chat holds its own grants ([`Refused::Held`]);
//! 3. the persona exists and is not a draft ([`Refused::NoPersona`], [`Refused::Draft`]);
//! 4. policy: a policy file that is refused switches dispatch off, and a policy may lock all
//!    dispatch or this pair ([`Refused::Limit`] with the off sentence, [`Refused::Locked`]);
//! 5. a profile to start the persona chat on ([`Refused::Profile`]);
//! 6. the limits ([`Refused::Limit`]): a limit of 0, the loop rule, the depth, then how many
//!    the asking chat has running, how many its lineage holds, and the two counts a persona
//!    has;
//! 7. the grant: none is needed when the person dispatches, **who is not held to their own
//!    never either** (it is their rule for chats). For a chat: a pair the person said never
//!    to, for its own persona or for one above it in its chain, is refused and nobody is asked
//!    ([`Refused::Never`]); its own persona needs no grant; any other pair answers
//!    [`Decision::NeedsGrant`] until one is in force, and also while this machine's record of
//!    nevers does not read, when no grant counts.
//!
//! A limit is said before a grant is asked for: asking the person for a grant that would
//! start nothing wastes their yes.
//!
//! The spec's last step is not built yet (#1467): a start that waits while the machine is
//! short on memory. Nothing in purlis reads the machine's memory today, so a dispatch the
//! checks above allow starts at once.

use crate::dispatchgrant::Covers;
/// Where an asking chat stands among the chats the app has open, and the limits it is held
/// to: the limits module's own types, which this decision reads.
pub use crate::dispatchlimits::{DEEPEST, Limits, Lineage};
/// Why a dispatch is made: what the persona chat is for. One type with the record's
/// ([`crate::reopen::Mode`]), which is where a chat keeps it.
pub use crate::reopen::Mode;

/// The asking chat, as the app's record has it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AskingChat<'a> {
    /// The persona it runs as: its own, else the one a new chat adopts by default. `None` is a
    /// chat on no persona.
    pub persona: Option<&'a str>,
    /// Whether it holds another persona's grants instead of its own (#1362), which the person
    /// has not yet allowed away. Such a chat's own persona is not yet its own to pass on.
    pub held: bool,
}

/// Who asks for a dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asker<'a> {
    /// A chat the app has open.
    Chat(AskingChat<'a>),
    /// A harness's helper sub-agent, inside a chat. It is not a chat: nobody can see it, open
    /// it or stop it apart from its chat.
    Helper,
    /// The person, from this chat's tab (#1438). No grant is needed: the person is the one a
    /// grant is asked of.
    Person(AskingChat<'a>),
}

/// Whether a chat asks or the person does, from that chat's tab: what a caller of
/// [`asked_by_a_chat`] says of who is asking.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum By {
    /// The chat itself: its command or its tool.
    #[default]
    Chat,
    /// The person, from the chat's tab (#1438).
    Person,
}

/// The persona a dispatch names, as the project defines it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persona<'a> {
    /// Defined, and finished.
    Defined(&'a str),
    /// Defined, and still a draft (`draft: true`).
    Draft(&'a str),
    /// Not one this project defines, or one that does not load.
    Unknown(&'a str),
    /// No persona: a chat on none dispatching to "its own".
    None,
}

impl<'a> Persona<'a> {
    fn name(self) -> Option<&'a str> {
        match self {
            Self::Defined(name) | Self::Draft(name) | Self::Unknown(name) => Some(name),
            Self::None => None,
        }
    }
}

/// One dispatch, as the app asks about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request<'a> {
    pub asker: Asker<'a>,
    /// The persona the new chat would run as.
    pub to: Persona<'a>,
    pub mode: Mode,
    /// What the grants in force and this machine's policy say of the asking chat's persona
    /// dispatching to [`Self::to`] ([`crate::dispatchgrant::covers`]).
    pub grant: &'a Covers,
    /// Why no profile was chosen for the new chat, where none was
    /// ([`crate::personaprofile::for_dispatch`]).
    pub profile: Option<&'a crate::personaprofile::Refused>,
    /// The limits in force for this dispatch ([`crate::dispatchlimits::of`]).
    pub limits: &'a Limits,
    /// Where the asking chat stands among the chats the app has open ([`lineage_of`]).
    pub lineage: &'a Lineage,
}

/// What the app does with a dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Start the persona chat.
    Start,
    /// Start nothing yet, and ask the person for a dispatch grant for this pair.
    NeedsGrant {
        /// The asking chat's persona, or `None` for a chat on none.
        from: Option<String>,
        /// The persona it asked for.
        to: String,
    },
    /// Start nothing: this dispatch is not one a grant could allow.
    Refused(Refused),
}

/// Why a dispatch is refused. Each says what to do in [`Refused::say`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// A helper sub-agent asked.
    Helper,
    /// The asking chat holds another persona's grants instead of its own (#1362), which the
    /// person has not yet allowed away: it has no persona of its own to dispatch as or from.
    Held,
    /// No persona was named, and the asking chat's own is one.
    NoPersonaNamed,
    /// The project defines no such persona, or it does not load.
    NoPersona(String),
    /// The persona is a draft.
    Draft(String),
    /// An administrator's policy locks this dispatch: the policy's own sentence, which names
    /// who set it ([`crate::sandbox::policy::Locks::dispatch_refused`]). No grant covers it.
    Locked(String),
    /// No profile to start the persona chat on, and why ([`crate::personaprofile::Refused`]).
    Profile(crate::personaprofile::Refused),
    /// A limit or the loop rule stands in the way: which, with the count it stands at
    /// ([`crate::dispatchlimits::Refused`]).
    Limit(crate::dispatchlimits::Refused),
    /// The person said never to this dispatch (#1503): to the asking chat's persona, or to one
    /// above it in its chain. The whole sentence ([`crate::dispatchgrant::never_said`],
    /// [`crate::dispatchgrant::never_above_said`]). No grant covers it and nobody is asked.
    Never(String),
    /// The asking chat, by its number, is not among the chats the app has open when the
    /// decision is made (#1521): it closed or ended while its ask was on its way. Its chain and
    /// depth are on its own record, so nothing is decided without it.
    NotOpen(u32),
}

impl Refused {
    /// The sentence the asking chat reads: what was refused, and what to do instead.
    pub fn say(&self) -> String {
        match self {
            Self::Helper => HELPER.to_owned(),
            Self::Held => HELD.to_owned(),
            Self::NoPersonaNamed => "a dispatch names the persona its chat runs as. Name one \
                                     with --to, or leave it out for this chat's own."
                .to_owned(),
            Self::NoPersona(name) => format!(
                "this project has no persona '{}' that loads. List the personas with `purlis \
                 persona list`, then dispatch to one of them.",
                crate::shown::short(name)
            ),
            Self::Draft(name) => {
                let name = crate::shown::short(name);
                format!(
                    "persona '{name}' is still a draft, and a draft persona runs no chat. \
                     Finish its definition and drop its `draft: true` line, or dispatch to \
                     another persona."
                )
            }
            Self::Locked(why) => format!("{why} {LOCKED}"),
            Self::Profile(why) => why.say(),
            Self::Limit(why) => why.say(),
            Self::Never(said) => said.clone(),
            Self::NotOpen(chat) => format!("chat {chat} is not one this app has open"),
        }
    }
}

/// What a helper sub-agent that tries to dispatch is told ([`Refused::Helper`]). Spelled once:
/// the tool guard says it before the command runs, where a sub-agent is known for one.
pub const HELPER: &str = "a dispatch is refused from inside a helper. A task belongs to a chat \
     the person can see, open and stop, and a helper is not one. Return what you \
     found to your chat, and let that chat dispatch.";

/// What a chat that holds another persona's grants is told ([`Refused::Held`]).
pub const HELD: &str = "this chat runs with another persona's grants until the person allows \
     its own, so it cannot dispatch yet. Ask the person to allow this chat its own grants on \
     its tab, then dispatch again.";

/// What a chat is told to do about a dispatch a policy locks ([`Refused::Locked`]), after the
/// policy's own sentence: no grant lifts it, so asking the person again does nothing.
pub const LOCKED: &str = "No grant covers it, so do the work in this chat.";

/// A task's name as purlis will draw it, or why not.
///
/// A chat's name by the rule every chat's name is held to ([`crate::reopen::label`]), and not
/// empty. And none of the marks purlis's own lines are made of: a task's name is written into
/// the stamp of whatever its chat dispatches and into the heading of its report, between `⟨`
/// and `⟩`, beside `·` and inside a code span. A name holding one could close purlis's line
/// early and write the rest of it. **Refused, never stripped**, for the label's own reason.
pub fn task_name(raw: &str) -> Result<String, String> {
    let Some(name) = crate::reopen::label(raw)? else {
        return Err("a task needs a name, which its chat is called and listed under.".to_owned());
    };
    if name.contains(['⟨', '⟩', '·', '`']) {
        return Err(
            "That name holds one of ⟨ ⟩ · or a backtick, which purlis writes its own lines \
             with, so it will not draw them in a task's name."
                .to_owned(),
        );
    }
    Ok(name)
}

/// Who a dispatch is from and to, by the app's record of the asking chat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pair {
    /// The persona the asking chat runs as ([`crate::start::grants_persona`]): the grants it
    /// holds, else its own, else the one a new chat adopts by default. `None` is a chat on no
    /// persona. **Never a word of the request.**
    pub asking: Option<String>,
    /// The persona the new chat runs as: the one named, else the asking chat's own.
    pub to: Option<String>,
}

/// The pair a dispatch from the chat recorded as `asking` is across, naming `named` (or no
/// persona, for its own). `default` is the persona a new chat adopts.
///
/// One answer for every caller, so the profile is chosen for the same persona the decision
/// is made about.
pub fn pair_of(asking: &crate::reopen::Chat, named: Option<&str>, default: Option<&str>) -> Pair {
    let runs_as =
        crate::start::grants_persona(asking.held.as_ref(), asking.persona.as_deref(), || {
            default.map(str::to_owned)
        });
    // What a dispatch to "its own" names is the persona it is, held or not: a held chat is
    // refused before that matters, and the person dispatching from its tab means that one.
    let own = asking.persona.as_deref().or(default);
    let to = named
        .map(str::trim)
        .filter(|named| !named.is_empty())
        .or(own)
        .map(str::to_owned);
    Pair {
        asking: runs_as,
        to,
    }
}

/// What the app holds at the moment it decides a dispatch: read under its own lock, so two
/// asks in flight are decided one after the other.
#[derive(Clone, Copy)]
pub struct Moment<'a> {
    /// Every chat it has open, and every one it is about to start, by its number for each.
    pub open: &'a [(u32, &'a crate::reopen::Chat)],
    /// Whether a chat's program is still running, or about to.
    pub working: &'a dyn Fn(u32) -> bool,
    /// The persona a new chat adopts by default, which a chat that names none runs as.
    pub default: Option<&'a str>,
    /// The dispatch grants in force for the asking chat: its own, the person's on this
    /// machine, and the project's ([`crate::dispatchgrant::InForce::read`]).
    pub grants: &'a crate::dispatchgrant::InForce,
    /// Why no profile was chosen for the new chat, where none was.
    pub profile: Option<&'a crate::personaprofile::Refused>,
    /// Whether the chat asks, or the person does from its tab.
    pub by: By,
    /// Why the dispatch is made: a task, or a handoff (#1444). One decision answers both.
    pub mode: Mode,
    /// Which of the asking chat's own count as running: its handoffs too, for a chat nobody
    /// is at (D-1444-13).
    pub counted: Counted,
    /// The workspace the new chat is to work in, where the dispatch names one or cuts a
    /// worktree in one (#1453, [`crate::dispatchplace::Ground::workspace`]). `None` is the
    /// asking chat's own.
    pub works_in: Option<&'a str>,
}

/// What a chat's dispatch comes to: the decision, and the facts the app starts the persona
/// chat with when the decision is to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub decision: Decision,
    /// The persona the new chat runs as: the one named, else the asking chat's own.
    pub to: Option<String>,
    /// How deep the new chat is in its chain: one below the chat that asks.
    pub depth: u32,
    /// The stable id of the chat the person started, which the whole lineage descends from
    /// ([`crate::reopen::HandedFrom::root`]): what the new chat's record keeps.
    pub root: Option<String>,
    /// **The personas above the new chat, nearest first** (#1521): the asking chat's own, then
    /// its chain as its record keeps it ([`crate::reopen::HandedFrom::above`]), one for each
    /// of [`Self::depth`]. Read from the app's record of the asking chat, never from what it
    /// sent. `None` where purlis cannot read that chain whole, which the new chat's own asks
    /// are then held to ([`crate::dispatchlimits::Lineage::chain_unread`]).
    pub above: Option<Vec<Option<String>>>,
}

/// The decision for a dispatch the chat `number` asks for, naming `named` (or no persona, for
/// its own), in the project at `root`. The chat is read from `moment.open`, under the lock the
/// app decides under; one that is not there is refused ([`Refused::NotOpen`]).
///
/// **The one place the parts are joined.** Every fact about the asker is read here from the
/// app's record of it, so no caller builds a [`Request`] its own way: the persona it runs as,
/// whether it holds another's grants, and where it stands among `moment.open`. The limits in
/// force are read from the project's files and this machine's policy as they stand now
/// ([`crate::dispatchlimits::of`]), and the grant from `moment.grants` under the same policy
/// ([`crate::dispatchgrant::covers`]).
///
/// **A dispatch into another workspace is held to both workspaces' limits** (#1453): the one
/// the asking chat works in, as every dispatch is, and then the one the new chat is to work
/// in (`moment.works_in`). The first that refuses is the answer. Read for the asking chat's
/// alone, a workspace that switched dispatch off would still be worked in by any chat that
/// named it; read for the other alone, a chat in a workspace with dispatch off would dispatch
/// by naming another. **A 0 in the destination's own row holds against a persona's value too**
/// ([`crate::dispatchlimits::off_in`], D-1453-17): elsewhere a persona's row is the more
/// specific and wins.
pub fn asked_by_a_chat(
    root: &std::path::Path,
    number: u32,
    named: Option<&str>,
    moment: &Moment<'_>,
) -> Asked {
    // **Its own record, as the app holds it now** (#1521): the chain and depth a new chat
    // keeps are read from it under the lock the decision is made under. A chat that left
    // since `asking` was read would otherwise read as having nobody above it, and that
    // shorter chain would be written into the chat it starts.
    let Some(asking) = record(number, moment.open) else {
        return Asked {
            decision: Decision::Refused(Refused::NotOpen(number)),
            to: None,
            depth: 0,
            root: None,
            above: None,
        };
    };
    let pair = pair_of(asking, named, moment.default);
    let workspace = asking
        .cwd
        .as_deref()
        .and_then(|cwd| crate::active::workspace_of_tree(root, cwd));
    let limits = crate::dispatchlimits::of(
        root,
        workspace.as_deref(),
        pair.asking.as_deref(),
        pair.to.as_deref(),
    );
    let lineage = lineage_counting(
        number,
        moment.open,
        moment.default,
        moment.working,
        &pair,
        moment.counted,
    );
    let locks = crate::sandbox::policy::Locks::of(root);
    // **Where the task works** (#1505): the workspace the dispatch names or moves into, else
    // the asking chat's own. Said here, from the app's record of both, so a grant limited to
    // one workspace is judged against where the new chat will run whatever the caller read.
    let grants = moment
        .grants
        .clone()
        .for_task_in(crate::dispatchwithin::works_in(
            None,
            moment.works_in,
            workspace.as_deref(),
        ));
    let grant = match pair.to.as_deref() {
        // With who is above the asking chat, so a never said for one of them holds here too.
        Some(to) => crate::dispatchgrant::covers_in_chain(
            pair.asking.as_deref(),
            to,
            &grants,
            &locks,
            &lineage.chain,
        ),
        // A chat on no persona dispatching to none: its own, which only a lock on all
        // dispatch stands in the way of.
        None if locks.forbids_dispatch() => Covers::Locked(
            locks
                .dispatch_refused(None, "")
                .unwrap_or_else(|| locks.locked_by()),
        ),
        None => Covers::Covered,
    };
    let its = AskingChat {
        persona: pair.asking.as_deref(),
        held: asking.held.is_some(),
    };
    let request = Request {
        asker: match moment.by {
            By::Chat => Asker::Chat(its),
            By::Person => Asker::Person(its),
        },
        to: match pair.to.as_deref() {
            Some(name) => persona_in(root, name),
            None => Persona::None,
        },
        mode: moment.mode,
        grant: &grant,
        profile: moment.profile,
        limits: &limits,
        lineage: &lineage,
    };
    let mut decision = decide(&request);
    // **A chain it cannot read whole** (#1521) may hold a persona the person said never to for
    // this target: refused as if it did. The loop rule has refused every other persona already
    // ([`crate::dispatchlimits::Refused::ChainUnread`]); this is the chat's own. The person's own
    // dispatch from a tab is held to no never.
    if let (true, By::Chat, Some(to), Decision::Start | Decision::NeedsGrant { .. }) = (
        lineage.chain_unread,
        moment.by,
        pair.to.as_deref(),
        &decision,
    ) && grants.refuses_any_to(to)
    {
        decision = Decision::Refused(Refused::Limit(crate::dispatchlimits::Refused::ChainUnread(
            to.to_owned(),
        )));
    }
    let elsewhere = moment
        .works_in
        .filter(|there| Some(*there) != workspace.as_deref());
    if let (Some(there), false) = (elsewhere, matches!(decision, Decision::Refused(_))) {
        let theirs = crate::dispatchlimits::of(
            root,
            Some(there),
            pair.asking.as_deref(),
            pair.to.as_deref(),
        );
        let held_there = decide(&Request {
            limits: &theirs,
            ..request
        });
        // **Off at the destination is off** (D-1453-17), whatever the asking persona's own
        // row says: said first, since it is the plainer answer.
        if let Some(off) = crate::dispatchlimits::off_in(root, there, pair.to.as_deref()) {
            decision = Decision::Refused(Refused::Limit(off));
        } else if matches!(held_there, Decision::Refused(_)) {
            decision = held_there;
        }
    }
    let depth = lineage.depth.saturating_add(1).min(DEEPEST);
    Asked {
        decision,
        depth,
        root: root_of(number, moment.open),
        above: above_the_new_chat(asking, moment.default, &lineage, depth),
        to: pair.to,
    }
}

/// The personas above a chat `asking` dispatches, nearest first: its own persona (the
/// default for a chat on none), then the chain above it. Kept only where it is whole and one
/// per dispatch above the new chat, `depth` of them; otherwise `None`, a chain not kept, which
/// is read as one purlis cannot read whole and never as a shorter one (#1521).
fn above_the_new_chat(
    asking: &crate::reopen::Chat,
    default: Option<&str>,
    lineage: &Lineage,
    depth: u32,
) -> Option<Vec<Option<String>>> {
    if lineage.chain_unread {
        return None;
    }
    let own = asking
        .persona
        .clone()
        .or_else(|| default.map(str::to_owned));
    let above: Vec<Option<String>> = std::iter::once(own)
        .chain(lineage.chain.iter().cloned())
        .collect();
    (usize::try_from(depth).ok() == Some(above.len())).then_some(above)
}

/// Whether the dispatch `request` describes may start.
pub fn decide(request: &Request<'_>) -> Decision {
    use crate::dispatchlimits::{self, Source};
    let refused = |why| Decision::Refused(why);
    // 1. Who asks.
    let (asking, by_the_person) = match request.asker {
        Asker::Helper => return refused(Refused::Helper),
        Asker::Chat(asking) => (asking, false),
        Asker::Person(asking) => (asking, true),
    };
    // 2. Grants of its own to dispatch with: a chat still holding another persona's has none
    // (#1362), whatever it asks for. The person dispatching from its tab is not held to that:
    // the hold is theirs to lift.
    if asking.held && !by_the_person {
        return refused(Refused::Held);
    }
    // 3. The persona.
    match request.to {
        Persona::Unknown(name) => return refused(Refused::NoPersona(name.to_owned())),
        Persona::Draft(name) => return refused(Refused::Draft(name.to_owned())),
        Persona::Defined(_) | Persona::None => {}
    }
    let to = request.to.name();
    // 4. Policy, which holds the person too. A policy file that is refused says nothing
    // purlis can read, so dispatch is off and the sentence says the file is refused; one that
    // is read may lock all dispatch, or this pair.
    let within = dispatchlimits::decide(request.limits, request.lineage);
    if let Some(
        off @ dispatchlimits::Refused::Off {
            by: Source::PolicyRefused,
            ..
        },
    ) = within.refused()
    {
        return refused(Refused::Limit(off.clone()));
    }
    if let Covers::Locked(why) = request.grant {
        return refused(Refused::Locked(why.clone()));
    }
    // 5. A profile to start it on.
    if let Some(why) = request.profile {
        return refused(Refused::Profile(why.clone()));
    }
    // 6. The limits: a 0, the loop rule, the depth, then the counts.
    if let Some(why) = within.refused() {
        return refused(Refused::Limit(why.clone()));
    }
    // 7. The grant. The person needs none; a chat's own persona needs none, which is what
    // `covers` answers for it; a pair the person granted has one.
    if by_the_person {
        return Decision::Start;
    }
    match (to, request.grant) {
        // No persona named, for a chat that runs as one: not a pair a grant could name.
        (None, _) if asking.persona.is_some() => refused(Refused::NoPersonaNamed),
        (_, Covers::Covered) => Decision::Start,
        // The person's never: refused here, so no reader of this decision asks them.
        (Some(to), Covers::Never) => refused(Refused::Never(crate::dispatchgrant::never_said(
            asking.persona.unwrap_or_default(),
            to,
        ))),
        (Some(to), Covers::NeverAbove(above)) => refused(Refused::Never(
            crate::dispatchgrant::never_above_said(above, to),
        )),
        // No grant, or none that counts while the record of nevers does not read: the person
        // is asked. Policy was answered at step 4.
        (Some(to), Covers::NeedsGrant | Covers::Unread | Covers::Locked(_)) => {
            Decision::NeedsGrant {
                from: asking.persona.map(str::to_owned),
                to: to.to_owned(),
            }
        }
        (None, _) => refused(Refused::NoPersonaNamed),
    }
}

// ----------------------------------------------------------------------------------------
// the facts a request is made of, read from what the app holds
// ----------------------------------------------------------------------------------------

/// `name` as the project at `root` defines it: the persona a dispatch names, asked of the
/// definition and never of the request ([`crate::personas::name_refusal`], the answer every
/// command that takes a persona's name gives).
pub fn persona_in<'a>(root: &std::path::Path, name: &'a str) -> Persona<'a> {
    if crate::personas::name_refusal(root, name).is_some() {
        Persona::Unknown(name)
    } else if crate::personaverbs::is_draft(root, name) {
        Persona::Draft(name)
    } else {
        Persona::Defined(name)
    }
}

/// The record of chat `number` among `open`.
fn record<'a>(
    number: u32,
    open: &'a [(u32, &'a crate::reopen::Chat)],
) -> Option<&'a crate::reopen::Chat> {
    open.iter()
        .find(|(n, _)| *n == number)
        .map(|(_, chat)| *chat)
}

/// The stable id of the chat the person started, which chat `number`'s lineage descends from:
/// the one its record was given when it was dispatched, or its own id for a chat nobody
/// dispatched. `None` for a chat with no id yet, and for a record written before the key.
pub fn root_of(number: u32, open: &[(u32, &crate::reopen::Chat)]) -> Option<String> {
    let chat = record(number, open)?;
    match &chat.from {
        Some(from) => from.root.clone(),
        None => chat.identity.id.clone(),
    }
}

/// The lineage of chat `asking` among `open`, the chats the app has open by its number for
/// each, for a dispatch across `pair`. `default` is the persona a chat that names none runs
/// as, and `working` whether a chat's program is still running.
///
/// **Read from the app's records and nothing else.** A chat's depth is the one its own record
/// holds, written when it was dispatched, so closing the chat above it never makes a chain
/// look shallower. **So is its chain** (#1521): the personas above it as its record keeps
/// them ([`crate::reopen::HandedFrom::above`]), so a chat above it that closed, finished or
/// was cleared is still in it. A chat whose record names no asking chat, the person's own or a
/// finished task reopened as an ordinary chat, has nothing above it.
///
/// **A record written before the chain was kept** is read by walking who dispatched whom
/// among the chats still open, as before, up to a chat that names no asking chat or one whose
/// record keeps its chain. Where the walk meets a chat that has closed, or a loop no dispatch
/// made, purlis cannot say who is above: [`Lineage::chain_unread`].
///
/// **A lineage is counted by its root** ([`root_of`]): every open chat whose record names the
/// same chat the person started is in it, so closing a chat in the middle, or starting one
/// again under a new number, never splits it in two. A record written before a root was kept
/// is found by walking who dispatched whom, as before.
///
/// **The counts are of chats that still owe work** (D-1436-18): one that has not reported and
/// whose program has not ended. A chat that reported is ended once that turn is over (#1485)
/// and costs nothing; one whose program ended without a report has failed, and is not running.
pub fn lineage_of(
    asking: u32,
    open: &[(u32, &crate::reopen::Chat)],
    default: Option<&str>,
    working: &dyn Fn(u32) -> bool,
    pair: &Pair,
) -> Lineage {
    lineage_counting(asking, open, default, working, pair, Counted::Tasks)
}

/// Which of the chats an asking chat opened count toward its running-per-chat limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Counted {
    /// Its tasks: the work it waits on. A handoff's work is not the asking chat's to wait on.
    #[default]
    Tasks,
    /// Its handoffs as well, **for a chat nobody is at** (D-1444-13): nobody watches what it
    /// opens, so each chat it opened that still works is held to the one limit, whichever
    /// mode opened it.
    HandoffsToo,
}

/// [`lineage_of`], with `which` saying which of the asking chat's own count as running.
pub fn lineage_counting(
    asking: u32,
    open: &[(u32, &crate::reopen::Chat)],
    default: Option<&str>,
    working: &dyn Fn(u32) -> bool,
    pair: &Pair,
    which: Counted,
) -> Lineage {
    let record = |number: u32| record(number, open);
    let asker_of = |number: u32| record(number)?.from.as_ref().map(|from| from.chat);
    let persona =
        |chat: &crate::reopen::Chat| chat.persona.clone().or_else(|| default.map(str::to_owned));
    // Whose grants a chat runs with, which is who it dispatches as.
    let runs_as = |chat: &crate::reopen::Chat| {
        crate::start::grants_persona(chat.held.as_ref(), chat.persona.as_deref(), || {
            default.map(str::to_owned)
        })
    };
    // Up: the chats above it that are still open, nearest first, for the counts below. `seen`
    // ends a loop that only a record somebody else wrote could hold.
    let mut seen = vec![asking];
    let mut walked = Vec::new();
    // What the walk found of the chain for a record that keeps none: the first chat above
    // whose record keeps its own, by how many it had walked to reach it.
    let mut kept_above: Option<(usize, Vec<Option<String>>)> = None;
    // Whether the walk stopped short of the chat the person started: at a chat that closed,
    // or at a loop.
    let mut short = false;
    let mut top = asking;
    while let Some(above) = asker_of(top) {
        let Some(chat) = record(above).filter(|_| !seen.contains(&above)) else {
            short = true;
            break;
        };
        walked.push(persona(chat));
        if kept_above.is_none()
            && let Some(kept) = chat.from.as_ref().and_then(|from| from.above.clone())
        {
            kept_above = Some((walked.len(), kept));
        }
        seen.push(above);
        top = above;
    }
    // The chain: the one the asking chat's own record keeps; for a record written before it
    // was kept, the walk, finished by the first record above it that keeps its own.
    let own = record(asking).and_then(|chat| chat.from.as_ref());
    let (chain, chain_unread) = match (own.map(|from| from.above.clone()), kept_above) {
        (None, _) => (Vec::new(), false),
        (Some(Some(kept)), _) => (kept, false),
        (Some(None), Some((reached, kept))) => {
            walked.truncate(reached);
            walked.extend(kept);
            (walked, false)
        }
        (Some(None), None) => (walked, short),
    };
    // Every open chat of the same root, then down from the top: every open chat that descends
    // from it, itself included.
    let root = root_of(asking, open);
    let mut lineage = vec![top];
    if root.is_some() {
        for (number, _) in open {
            if root_of(*number, open) == root && !lineage.contains(number) {
                lineage.push(*number);
            }
        }
    }
    let mut at = 0;
    while at < lineage.len() {
        let above = lineage[at];
        for (number, _) in open {
            if asker_of(*number) == Some(above) && !lineage.contains(number) {
                lineage.push(*number);
            }
        }
        at += 1;
    }
    // Still owing work: its program runs, and it has not sent the report it owes.
    let owes_work = |number: u32| {
        working(number)
            && record(number).is_some_and(|chat| {
                chat.from.as_ref().is_none_or(|from| {
                    // Reported, by itself or by the app in its place (#1443).
                    !matches!(
                        from.report,
                        crate::reopen::Owed::Sent | crate::reopen::Owed::Failed
                    )
                })
            })
    };
    let counted = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    // A task some chat dispatched that still owes it work. A handoff's work is not the asking
    // chat's to wait on.
    let task_of = |number: u32, chat: &crate::reopen::Chat| {
        chat.from
            .as_ref()
            .filter(|from| from.mode == Mode::Task && owes_work(number))
            .map(|from| from.chat)
    };
    // What the asking chat opened that still works: its tasks, and where `which` says so
    // its handoffs too.
    let opened_by = |number: u32, chat: &crate::reopen::Chat| {
        chat.from
            .as_ref()
            .filter(|from| {
                (from.mode == Mode::Task || which == Counted::HandoffsToo) && owes_work(number)
            })
            .map(|from| from.chat)
    };
    let running = open
        .iter()
        .filter(|(number, chat)| *number != asking && opened_by(*number, chat) == Some(asking))
        .count();
    // Across the project: the chats running as the target persona, and the tasks the chats
    // running as the asking persona still wait on, this chat's own among them.
    let as_target = match pair.to.as_deref() {
        Some(target) => open
            .iter()
            .filter(|(number, chat)| owes_work(*number) && persona(chat).as_deref() == Some(target))
            .count(),
        None => 0,
    };
    let by_asking = match pair.asking.as_deref() {
        Some(asker) => open
            .iter()
            .filter(|(number, chat)| {
                task_of(*number, chat)
                    .and_then(record)
                    .is_some_and(|by| runs_as(by).as_deref() == Some(asker))
            })
            .count(),
        None => 0,
    };
    let lineage = lineage
        .into_iter()
        .filter(|number| owes_work(*number))
        .count();
    Lineage {
        depth: record(asking)
            .and_then(|chat| chat.from.as_ref())
            .map_or(0, |from| from.depth),
        chain,
        chain_unread,
        running: counted(running),
        lineage: counted(lineage),
        as_target: counted(as_target),
        by_asking: counted(by_asking),
    }
}

/// Where a persona chat stands, as its asking chat sees it (#1443): what a chat's own list of
/// the tasks it dispatched says of each, and what the window marks a task with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// At work, and its report is still to come.
    Running,
    /// Stopped on something only the person can answer, in its own tab: a permission prompt,
    /// a question, a Notice. Its asking chat cannot answer for the person, and is told to wait.
    WaitingOnOperator,
    /// It sent its report. Its program is ended once the turn that sent it is over (#1485).
    Reported,
    /// Its program ended before it reported, and the asking chat was told `failed`.
    Ended,
}

impl Standing {
    /// The words a chat's list of its dispatches says.
    pub fn word(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::WaitingOnOperator => "waiting on the person",
            Self::Reported => "reported",
            Self::Ended => "ended without a report",
        }
    }

    /// Whether its asking chat still has it in flight: what closing that chat asks about.
    pub fn in_flight(self) -> bool {
        matches!(self, Self::Running | Self::WaitingOnOperator)
    }
}

/// Where a persona chat stands, from the app's own record of it: what it `owes` its asking
/// chat, whether its program has `ended`, and whether it `waits` on the person. Nothing the
/// persona chat says is read.
///
/// A report settles it whatever came after: a chat that reported and then stopped on a prompt
/// has reported. One that has not, and whose program is gone, has ended, before the app's own
/// `failed` report is even written.
pub fn standing(owes: crate::reopen::Owed, ended: bool, waits: bool) -> Standing {
    use crate::reopen::Owed;
    match owes {
        Owed::Sent => Standing::Reported,
        Owed::Failed => Standing::Ended,
        Owed::Due | Owed::Nothing if ended => Standing::Ended,
        Owed::Due | Owed::Nothing if waits => Standing::WaitingOnOperator,
        Owed::Due | Owed::Nothing => Standing::Running,
    }
}

/// The start of the persona chat a dispatch from `asking` opens on `profile`, under the name
/// `name`, holding `its`: what a covered dispatch's chat holds
/// ([`crate::dispatchgrant::grants_for_a_dispatched_chat`]), or `None` for a chat on no
/// persona dispatching to none.
///
/// **What it takes from the asking chat is the folder, and nothing else.** The profile is the
/// one chosen for it ([`crate::personaprofile::for_dispatch`]), which is the asking chat's
/// only where neither the dispatch nor the persona names another. Its sandbox is what the
/// project compiles for its own persona in that folder ([`crate::start::ready`]): the asking
/// chat's per-chat grants, the grants it holds, its opt-out and whatever its harness was
/// switched to while it ran are not the new chat's, and this never copies them (#1434, #1437).
pub fn start_for(
    asking: &crate::reopen::Chat,
    its: Option<crate::dispatchgrant::Dispatched>,
    profile: String,
    name: String,
) -> crate::start::Start {
    let (persona, held, grants, without_sandbox) = match its {
        Some(its) => (Some(its.persona), its.held, its.grants, its.without_sandbox),
        None => (None, None, crate::sandbox::grant::Grants::default(), None),
    };
    crate::start::Start {
        profile: Some(profile),
        persona,
        name,
        cwd: asking.cwd.clone(),
        resume: None,
        show_footer: false,
        resuming: None,
        without_sandbox,
        held,
        grants,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- the facts, from the app's records ------------------------------------------------

    use crate::dispatchgrant::InForce;
    use crate::dispatchlimits::{self, Level, Limit, Source, Table};
    use crate::reopen::{Chat, HandedFrom, HeldGrants, Identity, Owed};

    const ROOT_ID: &str = "01J9ZQ3V5N8X4T2K7M6P0R1S2A";
    const OTHER_ID: &str = "01J9ZQ3V5N8X4T2K7M6P0R1S2B";

    fn chat(persona: Option<&str>) -> Chat {
        Chat {
            program: "claude".to_owned(),
            name: "1".to_owned(),
            profile: Some("work".to_owned()),
            persona: persona.map(str::to_owned),
            ..Default::default()
        }
    }

    /// A chat the person started, with the id its lineage is counted by.
    fn started(persona: Option<&str>, id: &str) -> Chat {
        Chat {
            identity: Identity {
                id: Some(id.to_owned()),
                ..Default::default()
            },
            ..chat(persona)
        }
    }

    fn dispatched(by: u32, depth: u32, mode: Mode, report: Owed, persona: Option<&str>) -> Chat {
        Chat {
            from: Some(HandedFrom {
                chat: by,
                name: "steward 1".to_owned(),
                workspace: crate::active::Place::Workspace("alpha".to_owned()),
                report,
                mode,
                depth,
                root: None,
                above: None,
                by_person: false,
            }),
            ..chat(persona)
        }
    }

    /// [`dispatched`], in the lineage of the chat whose id is `root`.
    fn under(root: &str, by: u32, depth: u32, persona: Option<&str>) -> Chat {
        let mut chat = dispatched(by, depth, Mode::Task, Owed::Due, persona);
        if let Some(from) = chat.from.as_mut() {
            from.root = Some(root.to_owned());
        }
        chat
    }

    fn pair(asking: Option<&str>, to: Option<&str>) -> Pair {
        Pair {
            asking: asking.map(str::to_owned),
            to: to.map(str::to_owned),
        }
    }

    /// The lineage of `asking` among `open`, every chat still working, for a chat running as
    /// `steward` dispatching to its own persona.
    fn seen(asking: u32, open: &[(u32, &Chat)]) -> Lineage {
        lineage_of(
            asking,
            open,
            None,
            &|_| true,
            &pair(Some("steward"), Some("steward")),
        )
    }

    #[test]
    fn a_chat_the_person_started_stands_alone_at_depth_zero() {
        let one = chat(Some("steward"));
        assert_eq!(
            seen(1, &[(1, &one)]),
            Lineage {
                depth: 0,
                chain: Vec::new(),
                running: 0,
                lineage: 1,
                // Itself: one chat is running as the persona it would dispatch to.
                as_target: 1,
                by_asking: 0,
                chain_unread: false,
            }
        );
    }

    #[test]
    fn a_lineage_is_read_from_the_records_of_the_chats_that_are_open() {
        // 1 (steward) ── 2 (devops, task) ── 4 (devops, task)
        //             ├─ 3 (steward, task, reported)
        //             └─ 5 (qa, handoff)
        // 6 is another lineage.
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let three = dispatched(1, 1, Mode::Task, Owed::Sent, Some("steward"));
        let four = dispatched(2, 2, Mode::Task, Owed::Due, Some("devops"));
        let five = dispatched(1, 1, Mode::Handoff, Owed::Nothing, Some("qa"));
        let six = chat(Some("steward"));
        let open = [
            (1, &one),
            (2, &two),
            (3, &three),
            (4, &four),
            (5, &five),
            (6, &six),
        ];

        let first = seen(1, &open);
        assert_eq!((first.depth, first.chain.len()), (0, 0));
        // 2 is still working. 3 has reported, and 5 was handed off: neither is a task this
        // chat is waiting on.
        assert_eq!(first.running, 1);
        // And 3, having reported, is no longer one the lineage holds.
        assert_eq!(first.lineage, 4);

        let fourth = seen(4, &open);
        assert_eq!(fourth.depth, 2);
        assert_eq!(
            fourth.chain,
            vec![Some("devops".to_owned()), Some("steward".to_owned())]
        );
        assert_eq!((fourth.running, fourth.lineage), (0, 4));
        assert_eq!(seen(6, &open).lineage, 1);
    }

    /// D-1444-13: a chat nobody is at is held to its running-per-chat limit by its handoffs
    /// as by its tasks, since nothing else bounds how many it opens. A chat a person is at is
    /// not: a handoff's work is not that chat's to wait on.
    #[test]
    fn for_a_chat_nobody_is_at_a_handoff_it_opened_counts_as_running_as_a_task_does() {
        // 1 (steward) ── 2 (devops, task, working)
        //             ├─ 3 (steward, task, reported)
        //             ├─ 5 (qa, handoff, working)
        //             └─ 7 (qa, handoff, its program ended)
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let three = dispatched(1, 1, Mode::Task, Owed::Sent, Some("steward"));
        let five = dispatched(1, 1, Mode::Handoff, Owed::Nothing, Some("qa"));
        let seven = dispatched(1, 1, Mode::Handoff, Owed::Nothing, Some("qa"));
        let open = [(1, &one), (2, &two), (3, &three), (5, &five), (7, &seven)];
        let own = pair(Some("steward"), Some("steward"));
        let working = |number: u32| number != 7;

        let attended = lineage_counting(1, &open, None, &working, &own, Counted::Tasks);
        assert_eq!(attended.running, 1, "the task alone");
        assert_eq!(attended, lineage_of(1, &open, None, &working, &own));

        let unattended = lineage_counting(1, &open, None, &working, &own, Counted::HandoffsToo);
        assert_eq!(
            unattended.running, 2,
            "the task, and the handoff still working"
        );
        // Nothing else it reads moves.
        assert_eq!(
            Lineage {
                running: attended.running,
                ..unattended.clone()
            },
            attended
        );
        // And it is refused at the limit, in the limit's own sentence.
        let mut limits = defaults(Some("steward"), Some("steward"));
        limits.running = 2;
        assert!(matches!(
            crate::dispatchlimits::decide(&limits, &unattended).refused(),
            Some(crate::dispatchlimits::Refused::TooManyRunning {
                limit: 2,
                running: 2
            })
        ));
        assert_eq!(
            crate::dispatchlimits::decide(&limits, &attended).refused(),
            None
        );
    }

    #[test]
    fn the_two_counts_a_persona_has_are_read_across_every_chat_of_the_project() {
        // Two lineages. 1 and 6 both run as steward; 2 and 7 are the tasks they wait on, and 3
        // is one that reported. 8 is a devops chat the person started, in another lineage.
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let three = dispatched(1, 1, Mode::Task, Owed::Sent, Some("devops"));
        let six = chat(Some("steward"));
        let seven = dispatched(6, 1, Mode::Task, Owed::Due, Some("qa"));
        let eight = chat(Some("devops"));
        let open = [
            (1, &one),
            (2, &two),
            (3, &three),
            (6, &six),
            (7, &seven),
            (8, &eight),
        ];
        let to_devops = pair(Some("steward"), Some("devops"));

        let all = lineage_of(1, &open, None, &|_| true, &to_devops);
        // `may-run-at-once` counts 2 and 8: 3 reported, and owes nothing more.
        assert_eq!(all.as_target, 2);
        // `may-dispatch` counts 2 and 7: what chats running as steward wait on, in the project.
        assert_eq!(all.by_asking, 2);
        // What this chat itself waits on is still its own.
        assert_eq!(all.running, 1);

        // A chat whose program ended is not running as anything, and nobody waits on it.
        let ended = lineage_of(1, &open, None, &|number| number != 2, &to_devops);
        assert_eq!((ended.as_target, ended.by_asking), (1, 1));

        // A chat on no persona runs as the default one, and is counted as it.
        let plain = chat(None);
        let with_plain = [(1, &one), (9, &plain)];
        assert_eq!(
            lineage_of(1, &with_plain, Some("devops"), &|_| true, &to_devops).as_target,
            1
        );
        // A dispatch that names no persona has neither count.
        let none = lineage_of(1, &open, None, &|_| true, &pair(None, None));
        assert_eq!((none.as_target, none.by_asking), (0, 0));
    }

    #[test]
    fn a_lineage_is_counted_by_its_root_so_a_chat_closed_in_the_middle_does_not_split_it() {
        // 1 ── 2 ── 4 and 1 ── 3, each record naming the chat the person started. Chat 2 has
        // closed: 4's asker is no longer open, and 4 is still in 1's lineage.
        let one = started(Some("steward"), ROOT_ID);
        let three = under(ROOT_ID, 1, 1, Some("steward"));
        let four = under(ROOT_ID, 2, 2, Some("steward"));
        let other = started(Some("steward"), OTHER_ID);
        let its_task = under(OTHER_ID, 5, 1, Some("steward"));
        let open = [
            (1, &one),
            (3, &three),
            (4, &four),
            (5, &other),
            (6, &its_task),
        ];

        assert_eq!(seen(1, &open).lineage, 3, "1, 3 and 4");
        assert_eq!(seen(4, &open).lineage, 3, "asked from below the gap");
        assert_eq!(seen(3, &open).lineage, 3);
        // The other lineage is its own.
        assert_eq!(seen(5, &open).lineage, 2);
        assert_eq!(root_of(4, &open).as_deref(), Some(ROOT_ID));
        assert_eq!(root_of(5, &open).as_deref(), Some(OTHER_ID));
    }

    #[test]
    fn a_lineage_whose_root_was_started_again_under_a_new_number_is_still_one() {
        // Chat 1 was started again as chat 9 (a restart keeps its id; the app rewrites the
        // number its tasks name). Its tasks, and theirs, are still its lineage.
        let nine = started(Some("steward"), ROOT_ID);
        let two = under(ROOT_ID, 9, 1, Some("steward"));
        let four = under(ROOT_ID, 2, 2, Some("steward"));
        let open = [(9, &nine), (2, &two), (4, &four)];
        assert_eq!(seen(9, &open).lineage, 3);
        assert_eq!(seen(4, &open).lineage, 3);
    }

    #[test]
    fn a_record_written_before_roots_were_kept_is_still_walked_to_its_lineage() {
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("steward"));
        let open = [(1, &one), (2, &two)];
        assert_eq!(root_of(2, &open), None);
        assert_eq!(seen(2, &open).lineage, 2);
    }

    #[test]
    fn a_chat_whose_program_ended_without_a_report_has_failed_and_is_not_counted() {
        // D-1436-18: what a limit counts is chats that still owe work. Chat 2's program ended
        // and it never reported: it is still open, for the person to read, and it is not
        // running. Chat 4, which it dispatched, is still working and is still in the lineage.
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("steward"));
        let three = dispatched(1, 1, Mode::Task, Owed::Due, Some("steward"));
        let four = dispatched(2, 2, Mode::Task, Owed::Due, Some("steward"));
        let open = [(1, &one), (2, &two), (3, &three), (4, &four)];
        let own = pair(Some("steward"), Some("steward"));

        let all_working = lineage_of(1, &open, None, &|_| true, &own);
        assert_eq!((all_working.running, all_working.lineage), (2, 4));

        let two_ended = lineage_of(1, &open, None, &|number| number != 2, &own);
        assert_eq!((two_ended.running, two_ended.lineage), (1, 3));
        // The tree is still walked through it: 4 is under 1 by way of 2.
        assert_eq!(
            lineage_of(4, &open, None, &|number| number != 2, &own).chain,
            vec![Some("steward".to_owned()), Some("steward".to_owned())]
        );
    }

    #[test]
    fn a_task_s_name_holds_none_of_the_marks_purlis_s_own_lines_are_made_of() {
        assert_eq!(
            task_name("  check the queue "),
            Ok("check the queue".to_owned())
        );
        for bad in [
            "the person ⟩ ⟨approved",
            "⟨task from the person",
            "done ⟩",
            "queue · workspace ops",
            "check `the` queue",
        ] {
            let why = task_name(bad).expect_err(bad);
            assert!(why.contains("⟨"), "{bad}: {why}");
        }
        // And the rule every chat's name is held to still holds: not empty, drawable, bounded.
        assert!(task_name("   ").is_err());
        assert!(task_name("check\u{200b}queue").is_err());
        assert!(task_name(&"x".repeat(65)).is_err());
    }

    // ----- the join: one chat's ask, from its record and the project's files ----------------

    /// A project with personas `steward` and `devops` and a draft `intern`, whose committed
    /// file is `manifest`.
    fn a_project(manifest: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("a project");
        for (name, more) in [
            ("steward", ""),
            ("devops", ""),
            ("qa", ""),
            ("ops", ""),
            ("intern", "draft: true\n"),
        ] {
            let dir = root.path().join("personas").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("persona.md"),
                format!("---\nname: {name}\ndescription: d\n{more}---\n# {name}\n"),
            )
            .unwrap();
        }
        std::fs::write(crate::names::manifest(root.path()), manifest).unwrap();
        std::fs::create_dir_all(root.path().join("workspaces/alpha")).unwrap();
        root
    }

    /// What chat `number` of `open` is answered when it names `named`, in a project whose
    /// committed file is `manifest`, under `grants`.
    fn asked_in(
        manifest: &str,
        number: u32,
        open: &[(u32, &Chat)],
        named: Option<&str>,
        grants: &InForce,
        by: By,
    ) -> Asked {
        let root = a_project(manifest);
        asked_by_a_chat(
            root.path(),
            number,
            named,
            &Moment {
                open,
                working: &|_| true,
                default: None,
                grants,
                profile: None,
                by,
                mode: Mode::Task,
                counted: Counted::Tasks,
                works_in: None,
            },
        )
    }

    /// One chat, alone, asking in a project with no limits of its own and no grants.
    fn asked(asking: &Chat, named: Option<&str>, default: Option<&str>) -> Asked {
        let root = a_project("");
        asked_by_a_chat(
            root.path(),
            2,
            named,
            &Moment {
                open: &[(2, asking)],
                working: &|_| true,
                default,
                grants: &InForce::default(),
                profile: None,
                by: By::Chat,
                mode: Mode::Task,
                counted: Counted::Tasks,
                works_in: None,
            },
        )
    }

    fn refusal(asked: &Asked) -> String {
        match &asked.decision {
            Decision::Refused(why) => why.say(),
            other => panic!("refused, not {other:?}"),
        }
    }

    #[test]
    fn a_chats_ask_is_built_from_its_record_and_names_its_own_persona_when_none_is_named() {
        // Dispatched by a chat on no persona, which has since closed.
        let steward = kept(1, &[None], Some("steward"));
        assert_eq!(
            asked(&steward, None, None),
            Asked {
                decision: Decision::Start,
                to: Some("steward".to_owned()),
                depth: 2,
                root: None,
                above: Some(vec![Some("steward".to_owned()), None]),
            }
        );
        // A name with blanks around it is the name; a blank one is none.
        assert_eq!(
            asked(&steward, Some(" steward "), None).decision,
            Decision::Start
        );
        assert_eq!(
            asked(&steward, Some("  "), None).to,
            Some("steward".to_owned())
        );
        assert_eq!(
            asked(&steward, Some("devops"), None).decision,
            Decision::NeedsGrant {
                from: Some("steward".to_owned()),
                to: "devops".to_owned(),
            }
        );
        assert_eq!(
            asked(&steward, Some("ghost"), None).decision,
            Decision::Refused(Refused::NoPersona("ghost".to_owned()))
        );
        assert_eq!(
            asked(&steward, Some("intern"), None).decision,
            Decision::Refused(Refused::Draft("intern".to_owned()))
        );
    }

    #[test]
    fn a_chat_on_no_persona_asks_as_the_default_and_with_no_default_as_none() {
        let plain = chat(None);
        let as_default = asked(&plain, None, Some("steward"));
        assert_eq!(as_default.decision, Decision::Start);
        assert_eq!(as_default.to.as_deref(), Some("steward"));
        let as_none = asked(&plain, None, None);
        assert_eq!(as_none.decision, Decision::Start);
        assert_eq!(as_none.to, None);
        // And a chat on none needs a grant for any persona, which only one for it covers.
        assert_eq!(
            asked(&plain, Some("devops"), None).decision,
            Decision::NeedsGrant {
                from: None,
                to: "devops".to_owned(),
            }
        );
    }

    #[test]
    fn a_new_chat_s_record_is_given_the_root_its_asking_chat_descends_from() {
        // The chat the person started: its own id is the root.
        let first = started(Some("steward"), ROOT_ID);
        assert_eq!(asked(&first, None, None).root.as_deref(), Some(ROOT_ID));
        // One that was dispatched: the root its own record names, never its own id.
        let below = Chat {
            identity: Identity {
                id: Some(OTHER_ID.to_owned()),
                ..Default::default()
            },
            ..under(ROOT_ID, 1, 1, Some("steward"))
        };
        assert_eq!(asked(&below, None, None).root.as_deref(), Some(ROOT_ID));
    }

    #[test]
    fn a_chat_holding_another_personas_grants_is_refused_whatever_it_names() {
        let holding = Chat {
            held: Some(HeldGrants {
                persona: Some("steward".to_owned()),
            }),
            ..chat(Some("devops"))
        };
        for named in [None, Some("devops"), Some("steward")] {
            assert_eq!(
                asked(&holding, named, None).decision,
                Decision::Refused(Refused::Held),
                "{named:?}"
            );
        }
    }

    #[test]
    fn the_asking_persona_is_whose_grants_the_chat_runs_with_and_never_a_word_it_sent() {
        // A chat's own persona, else the default; a held chat's is the one it holds.
        assert_eq!(
            pair_of(&chat(Some("steward")), Some("devops"), Some("qa")),
            pair(Some("steward"), Some("devops"))
        );
        assert_eq!(
            pair_of(&chat(None), None, Some("qa")),
            pair(Some("qa"), Some("qa"))
        );
        let holding = Chat {
            held: Some(HeldGrants {
                persona: Some("steward".to_owned()),
            }),
            ..chat(Some("devops"))
        };
        // "Its own" still names the persona it is, which is what the person means from its
        // tab; the grants it asks with are the ones it holds.
        assert_eq!(
            pair_of(&holding, None, None),
            pair(Some("steward"), Some("devops"))
        );
    }

    #[test]
    fn the_loop_rule_is_read_from_the_chain_the_records_hold() {
        // steward (1) dispatched devops (2). Devops asks for steward: from below itself.
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let open = [(1, &one), (2, &two)];
        let grants = InForce {
            you: vec![crate::dispatchgrant::Pair::new("devops", "steward").unwrap()],
            ..Default::default()
        };
        let said = asked_in("", 2, &open, Some("steward"), &grants, By::Chat);
        assert_eq!(
            said.decision,
            Decision::Refused(Refused::Limit(dispatchlimits::Refused::Loop(
                "steward".to_owned()
            )))
        );
        // Its own persona is not above it.
        assert_eq!(
            asked_in("", 2, &open, None, &grants, By::Chat).decision,
            Decision::Start
        );
    }

    #[test]
    fn the_limits_are_the_projects_own_read_for_the_asking_chats_workspace_and_persona() {
        // The project allows one running task a chat; the asking chat has one.
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("steward"));
        let open = [(1, &one), (2, &two)];
        let none = InForce::default();
        let said = asked_in(
            "[dispatch]\nrunning-per-chat = 1\n",
            1,
            &open,
            None,
            &none,
            By::Chat,
        );
        assert_eq!(
            refusal(&said),
            "this chat already has 1 task running, and it may have 1 at once. Wait \
             for one to report, then dispatch again."
        );
        // A persona's own row is more specific than the project's.
        let said = asked_in(
            "[dispatch]\nrunning-per-chat = 1\n[dispatch.personas.steward]\nrunning-per-chat = 4\n",
            1,
            &open,
            None,
            &none,
            By::Chat,
        );
        assert_eq!(said.decision, Decision::Start);
        // And the defaults hold where the project says nothing.
        assert_eq!(
            asked_in("", 1, &open, None, &none, By::Chat).decision,
            Decision::Start
        );
    }

    #[test]
    fn a_grant_for_one_workspace_is_judged_against_where_the_task_works_not_where_it_is_asked() {
        // #1505. `stands_in` is where the asking chat works (none: the project's root), and
        // `works_in` what the dispatch names; the grant is limited to `granted`.
        let ask = |stands_in: Option<&str>, works_in: Option<&str>, granted: &str| {
            let root = a_project("");
            std::fs::create_dir_all(root.path().join("workspaces/beta")).unwrap();
            let asking = Chat {
                cwd: stands_in.map(|name| root.path().join("workspaces").join(name)),
                ..chat(Some("steward"))
            };
            let grants = InForce {
                limited: vec![(
                    crate::sandbox::grant::Level::You,
                    crate::dispatchwithin::Limited::new("steward", "devops", granted).unwrap(),
                )],
                ..Default::default()
            };
            asked_by_a_chat(
                root.path(),
                1,
                Some("devops"),
                &Moment {
                    open: &[(1, &asking)],
                    working: &|_| true,
                    default: None,
                    grants: &grants,
                    profile: None,
                    by: By::Chat,
                    mode: Mode::Task,
                    counted: Counted::Tasks,
                    works_in,
                },
            )
            .decision
        };
        let needs = |decision: Decision| matches!(decision, Decision::NeedsGrant { .. });
        // A chat in alpha that names no place works in alpha.
        assert_eq!(ask(Some("alpha"), None, "alpha"), Decision::Start);
        assert!(needs(ask(Some("alpha"), None, "beta")));
        // It names beta: the task works in beta, and the grant for alpha does not go with it.
        assert!(needs(ask(Some("alpha"), Some("beta"), "alpha")));
        assert_eq!(ask(Some("alpha"), Some("beta"), "beta"), Decision::Start);
        // A chat anywhere may send the task into the workspace the grant names.
        assert_eq!(ask(None, Some("beta"), "beta"), Decision::Start);
        // A task at the project's root works in no workspace.
        assert!(needs(ask(None, None, "alpha")));
        // What the caller read the grants for does not change where the task works.
        let root = a_project("");
        let asking = Chat {
            cwd: Some(root.path().join("workspaces/alpha")),
            ..chat(Some("steward"))
        };
        let read_for_beta = InForce {
            limited: vec![(
                crate::sandbox::grant::Level::You,
                crate::dispatchwithin::Limited::new("steward", "devops", "beta").unwrap(),
            )],
            ..Default::default()
        }
        .for_task_in(Some("beta"));
        let said = asked_by_a_chat(
            root.path(),
            1,
            Some("devops"),
            &Moment {
                open: &[(1, &asking)],
                working: &|_| true,
                default: None,
                grants: &read_for_beta,
                profile: None,
                by: By::Chat,
                mode: Mode::Task,
                counted: Counted::Tasks,
                works_in: None,
            },
        );
        assert!(needs(said.decision));
    }

    #[test]
    fn a_dispatch_into_another_workspace_is_held_to_that_workspace_s_limits_and_its_own() {
        // #1453. The asking chat works in `alpha` and dispatches into `works_in`.
        let ask = |manifest: &str, works_in: Option<&str>| {
            let root = a_project(manifest);
            std::fs::create_dir_all(root.path().join("workspaces/beta")).unwrap();
            let asking = Chat {
                cwd: Some(root.path().join("workspaces/alpha")),
                ..chat(Some("steward"))
            };
            asked_by_a_chat(
                root.path(),
                1,
                None,
                &Moment {
                    open: &[(1, &asking)],
                    working: &|_| true,
                    default: None,
                    grants: &InForce::default(),
                    profile: None,
                    by: By::Chat,
                    mode: Mode::Task,
                    counted: Counted::Tasks,
                    works_in,
                },
            )
        };
        // The workspace it would work in has dispatch off: a chat elsewhere cannot work there
        // by naming it, and one that stays where it is is not held to it.
        let off_there = "[dispatch.workspaces.beta]\nrunning-per-chat = 0\n";
        assert_eq!(ask(off_there, None).decision, Decision::Start);
        assert_eq!(ask(off_there, Some("alpha")).decision, Decision::Start);
        let said = refusal(&ask(off_there, Some("beta")));
        assert!(
            said.starts_with("dispatch is off in the workspace beta: running per chat is set to 0"),
            "{said}"
        );
        // The asking chat's own workspace has it off: naming another is no way round it.
        let off_here = "[dispatch.workspaces.alpha]\nrunning-per-chat = 0\n";
        for works_in in [None, Some("beta")] {
            let said = refusal(&ask(off_here, works_in));
            assert!(
                said.starts_with("dispatch is off in the workspace alpha"),
                "{works_in:?}: {said}"
            );
        }
        // And where neither says anything, a dispatch into another workspace starts.
        assert_eq!(ask("", Some("beta")).decision, Decision::Start);
        // D-1453-17: a persona's own value does not lift the destination's 0, though it lifts
        // its own workspace's (ruling 9: the most specific level wins there).
        let persona = "[dispatch.personas.steward]\nrunning-per-chat = 3\n";
        let said = refusal(&ask(&format!("{off_there}{persona}"), Some("beta")));
        assert!(
            said.starts_with("dispatch is off in the workspace beta: running per chat is set to 0"),
            "{said}"
        );
        assert_eq!(
            ask(&format!("{off_here}{persona}"), Some("beta")).decision,
            Decision::Start
        );
        assert_eq!(
            ask(&format!("{off_here}{persona}"), None).decision,
            Decision::Start
        );
    }

    #[test]
    fn a_handoff_is_held_to_the_limits_of_the_workspace_it_moves_into_and_its_own() {
        // D-T61-7. The asking chat works in `alpha` and hands off into `works_in`: the same
        // two reads a task's `--in workspace:` gets, whoever is at the chat.
        let ask = |manifest: &str, works_in: Option<&str>, counted: Counted| {
            let root = a_project(manifest);
            std::fs::create_dir_all(root.path().join("workspaces/beta")).unwrap();
            let asking = Chat {
                cwd: Some(root.path().join("workspaces/alpha")),
                ..chat(Some("steward"))
            };
            asked_by_a_chat(
                root.path(),
                1,
                None,
                &Moment {
                    open: &[(1, &asking)],
                    working: &|_| true,
                    default: None,
                    grants: &InForce::default(),
                    profile: None,
                    by: By::Chat,
                    mode: Mode::Handoff,
                    counted,
                    works_in,
                },
            )
        };
        let off_there = "[dispatch.workspaces.beta]\nrunning-per-chat = 0\n";
        let off_here = "[dispatch.workspaces.alpha]\nrunning-per-chat = 0\n";
        let persona = "[dispatch.personas.steward]\nrunning-per-chat = 3\n";
        for counted in [Counted::Tasks, Counted::HandoffsToo] {
            // Off where it would move to: said, and a handoff that stays is not held to it.
            assert_eq!(
                ask(off_there, Some("alpha"), counted).decision,
                Decision::Start
            );
            let said = refusal(&ask(off_there, Some("beta"), counted));
            assert!(
                said.starts_with(
                    "dispatch is off in the workspace beta: running per chat is set to 0"
                ),
                "{said}"
            );
            // A persona's own value does not lift the destination's 0 (D-1453-17).
            let said = refusal(&ask(
                &format!("{off_there}{persona}"),
                Some("beta"),
                counted,
            ));
            assert!(
                said.starts_with("dispatch is off in the workspace beta"),
                "{said}"
            );
            // Off where the asking chat works: the first refusal is the answer.
            let said = refusal(&ask(off_here, Some("beta"), counted));
            assert!(
                said.starts_with("dispatch is off in the workspace alpha"),
                "{said}"
            );
            // And where neither says anything, it opens.
            assert_eq!(ask("", Some("beta"), counted).decision, Decision::Start);
        }
    }

    #[test]
    fn every_count_a_refusal_names_is_the_one_the_records_hold() {
        let none = InForce::default();
        // Depth: the asking chat is one dispatch down, and a chain may go one deep.
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("steward"));
        let open = [(1, &one), (2, &two)];
        assert_eq!(
            refusal(&asked_in(
                "[dispatch]\ndepth = 1\n",
                2,
                &open,
                None,
                &none,
                By::Chat
            )),
            "this chat is 1 dispatch below the chat the person started, and a chain may go 1 \
             deep here. Do the work in this chat, or say in your report what is left."
        );
        // The lineage: two chats still owe work, and it may hold two.
        assert_eq!(
            refusal(&asked_in(
                "[dispatch]\nlive-per-lineage = 2\n",
                1,
                &open,
                None,
                &none,
                By::Chat
            )),
            "this chat's lineage already holds 2 running chats, and it may hold 2. Wait for \
             one to finish, then dispatch again."
        );
        // What chats running as steward wait on between them, in the project.
        assert_eq!(
            refusal(&asked_in(
                "[dispatch.personas.steward]\nmay-dispatch = 1\n",
                1,
                &open,
                None,
                &none,
                By::Chat
            )),
            "chats as steward already have 1 task running between them, and may have \
             1 at once in this project. Wait for one to finish, then dispatch again."
        );
        // How many chats run as the target at once: both of these do.
        assert_eq!(
            refusal(&asked_in(
                "[dispatch.personas.steward]\nmay-run-at-once = 2\n",
                1,
                &open,
                None,
                &none,
                By::Chat
            )),
            "2 chats are already running as steward, and 2 may run as it at once in this \
             project. Wait for one to finish, then dispatch again."
        );
    }

    #[test]
    fn a_limit_of_zero_switches_dispatch_off_and_says_where() {
        let one = chat(Some("steward"));
        let said = asked_in(
            "[dispatch]\nrunning-per-chat = 0\n",
            1,
            &[(1, &one)],
            None,
            &InForce::default(),
            By::Chat,
        );
        assert_eq!(
            refusal(&said),
            "dispatch is off in this project: running per chat is set to 0. Only the person \
             can change it, in Settings › Project › Dispatch."
        );
    }

    #[test]
    fn a_grant_at_any_level_covers_the_pair_and_starts_it() {
        let one = chat(Some("steward"));
        let open = [(1, &one)];
        let to_devops = crate::dispatchgrant::Pair::new("steward", "devops").unwrap();
        let ask = |grants: &InForce| asked_in("", 1, &open, Some("devops"), grants, By::Chat);

        assert!(matches!(
            ask(&InForce::default()).decision,
            Decision::NeedsGrant { .. }
        ));
        for grants in [
            InForce {
                you: vec![to_devops.clone()],
                ..Default::default()
            },
            InForce {
                project: vec![to_devops.clone()],
                ..Default::default()
            },
            InForce {
                chat: vec![crate::dispatchgrant::ChatPair {
                    asking: Some("steward".to_owned()),
                    target: "devops".to_owned(),
                }],
                ..Default::default()
            },
        ] {
            assert_eq!(ask(&grants).decision, Decision::Start, "{grants:?}");
        }
        // A grant for another pair covers nothing here.
        let other = InForce {
            you: vec![crate::dispatchgrant::Pair::new("devops", "steward").unwrap()],
            ..Default::default()
        };
        assert!(matches!(ask(&other).decision, Decision::NeedsGrant { .. }));
    }

    #[test]
    fn the_person_dispatching_from_a_tab_needs_no_grant_and_is_held_to_the_limits() {
        let one = chat(Some("steward"));
        let open = [(1, &one)];
        let none = InForce::default();
        assert_eq!(
            asked_in("", 1, &open, Some("devops"), &none, By::Person).decision,
            Decision::Start
        );
        assert!(
            refusal(&asked_in(
                "[dispatch]\nrunning-per-chat = 0\n",
                1,
                &open,
                Some("devops"),
                &none,
                By::Person
            ))
            .starts_with("dispatch is off in this project")
        );
        // A chat that holds another persona's grants: the hold is the person's to lift, so
        // their own dispatch from its tab is not refused for it.
        let holding = Chat {
            held: Some(HeldGrants {
                persona: Some("steward".to_owned()),
            }),
            ..chat(Some("devops"))
        };
        assert_eq!(
            asked_in("", 1, &[(1, &holding)], Some("devops"), &none, By::Person).decision,
            Decision::Start
        );
    }

    #[test]
    fn a_policy_that_locks_the_pair_or_all_dispatch_refuses_with_who_locked_it() {
        use crate::sandbox::policy::{Locks, set_for_this_test};
        let one = chat(Some("steward"));
        let open = [(1, &one)];
        let granted = InForce {
            you: vec![crate::dispatchgrant::Pair::new("steward", "devops").unwrap()],
            ..Default::default()
        };
        let file = std::path::Path::new("/etc/purlis/policy.json");

        set_for_this_test(Locks::parse(
            r#"{"dispatch": {"locked": [{"from": "steward", "to": "devops"}]}}"#,
            file,
        ));
        // A grant already made covers nothing while the pair is locked, and the person's own
        // dispatch from the tab is locked too.
        for by in [By::Chat, By::Person] {
            let said = refusal(&asked_in("", 1, &open, Some("devops"), &granted, by));
            assert!(
                said.starts_with("Policy forbids steward chats dispatching to devops."),
                "{said}"
            );
            assert!(said.ends_with(LOCKED), "{said}");
        }
        // Its own persona is no pair, and starts.
        assert_eq!(
            asked_in("", 1, &open, None, &granted, By::Chat).decision,
            Decision::Start
        );

        set_for_this_test(Locks::parse(r#"{"dispatch": {"allow": false}}"#, file));
        for named in [None, Some("devops")] {
            let said = refusal(&asked_in("", 1, &open, named, &granted, By::Chat));
            assert!(
                said.starts_with("Policy forbids one chat dispatching to another."),
                "{said}"
            );
        }
        // A chat on no persona, dispatching to none, is held by it as any chat is.
        let plain = chat(None);
        let said = refusal(&asked_in(
            "",
            1,
            &[(1, &plain)],
            None,
            &InForce::default(),
            By::Chat,
        ));
        assert!(said.starts_with("Policy forbids one chat"), "{said}");
        set_for_this_test(Locks::none());
    }

    #[test]
    fn a_policy_s_ceiling_is_over_what_the_project_sets() {
        use crate::sandbox::policy::{Locks, set_for_this_test};
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("steward"));
        let open = [(1, &one), (2, &two)];
        set_for_this_test(Locks::parse(
            r#"{"dispatch": {"running-per-chat": 1}}"#,
            std::path::Path::new("/etc/purlis/policy.json"),
        ));
        let said = asked_in(
            "[dispatch]\nrunning-per-chat = 9\n",
            1,
            &open,
            None,
            &InForce::default(),
            By::Chat,
        );
        set_for_this_test(Locks::none());
        assert!(
            refusal(&said).contains("it may have 1 at once"),
            "{}",
            refusal(&said)
        );
    }

    // ----- the decision ---------------------------------------------------------------------

    fn steward() -> AskingChat<'static> {
        AskingChat {
            persona: Some("steward"),
            held: false,
        }
    }

    /// The limits in force where no file sets one, for `asking` dispatching to `target`.
    fn defaults(asking: Option<&str>, target: Option<&str>) -> Limits {
        dispatchlimits::in_force(
            &Table::default(),
            None,
            asking,
            target,
            &Table::default(),
            &Level::unset(),
        )
    }

    /// A chat the person started, with nothing running.
    fn alone() -> Lineage {
        Lineage {
            lineage: 1,
            ..Default::default()
        }
    }

    /// What `decide` says of a task from `asker` to `to`, under `grant`, the default limits
    /// and `lineage`.
    fn decided(asker: Asker<'_>, to: Persona<'_>, grant: &Covers, lineage: &Lineage) -> Decision {
        let asking = match asker {
            Asker::Chat(chat) | Asker::Person(chat) => chat.persona,
            Asker::Helper => None,
        };
        decide(&Request {
            asker,
            to,
            mode: Mode::Task,
            grant,
            profile: None,
            limits: &defaults(asking, to.name()),
            lineage,
        })
    }

    fn refused(decision: Decision) -> Refused {
        match decision {
            Decision::Refused(why) => why,
            other => panic!("refused, not {other:?}"),
        }
    }

    /// A handoff is a dispatch (#1444): the same askers, the same persona rules, the same
    /// limits and the same grant, whichever mode the request names.
    #[test]
    fn a_handoff_is_decided_as_a_task_is_at_every_step() {
        let devops = Persona::Defined("devops");
        let deep = Lineage {
            depth: 3,
            ..alone()
        };
        let locked = Covers::Locked("An administrator's policy locks it.".to_owned());
        let cases: [(Asker<'_>, Persona<'_>, &Covers, &Lineage); 8] = [
            // Its own persona: no grant, no prompt.
            (
                Asker::Chat(steward()),
                Persona::Defined("steward"),
                &Covers::Covered,
                &alone(),
            ),
            // Another persona: the grant, and with one it starts.
            (
                Asker::Chat(steward()),
                devops,
                &Covers::NeedsGrant,
                &alone(),
            ),
            (Asker::Chat(steward()), devops, &Covers::Covered, &alone()),
            // A helper sub-agent, a draft, a lock, the depth: refused, as a task is.
            (Asker::Helper, devops, &Covers::Covered, &alone()),
            (
                Asker::Chat(steward()),
                Persona::Draft("intern"),
                &Covers::Covered,
                &alone(),
            ),
            (Asker::Chat(steward()), devops, &locked, &alone()),
            (Asker::Chat(steward()), devops, &Covers::Covered, &deep),
            // The person, from the chat's tab: no grant.
            (
                Asker::Person(steward()),
                devops,
                &Covers::NeedsGrant,
                &alone(),
            ),
        ];
        for (asker, to, grant, lineage) in cases {
            let asking = match asker {
                Asker::Chat(chat) | Asker::Person(chat) => chat.persona,
                Asker::Helper => None,
            };
            let limits = defaults(asking, to.name());
            let as_a = |mode| {
                decide(&Request {
                    asker,
                    to,
                    mode,
                    grant,
                    profile: None,
                    limits: &limits,
                    lineage,
                })
            };
            assert_eq!(
                as_a(Mode::Handoff),
                decided(asker, to, grant, lineage),
                "{asker:?} to {to:?} under {grant:?}"
            );
        }
        // And said out, for the two the ticket names: none for the same persona, the grant
        // for another.
        let handoff = |to, grant: &Covers| {
            decide(&Request {
                asker: Asker::Chat(steward()),
                to,
                mode: Mode::Handoff,
                grant,
                profile: None,
                limits: &defaults(Some("steward"), to.name()),
                lineage: &alone(),
            })
        };
        assert_eq!(
            handoff(Persona::Defined("steward"), &Covers::Covered),
            Decision::Start
        );
        assert_eq!(
            handoff(devops, &Covers::NeedsGrant),
            Decision::NeedsGrant {
                from: Some("steward".to_owned()),
                to: "devops".to_owned(),
            }
        );
    }

    #[test]
    fn a_chat_dispatches_to_its_own_persona_with_no_grant() {
        assert_eq!(
            decided(
                Asker::Chat(steward()),
                Persona::Defined("steward"),
                &Covers::Covered,
                &alone()
            ),
            Decision::Start
        );
    }

    #[test]
    fn another_persona_needs_a_grant_and_starts_with_one() {
        let to = Persona::Defined("devops");
        assert_eq!(
            decided(Asker::Chat(steward()), to, &Covers::NeedsGrant, &alone()),
            Decision::NeedsGrant {
                from: Some("steward".to_owned()),
                to: "devops".to_owned(),
            }
        );
        assert_eq!(
            decided(Asker::Chat(steward()), to, &Covers::Covered, &alone()),
            Decision::Start
        );
    }

    #[test]
    fn a_chat_on_no_persona_dispatches_to_none_and_needs_a_grant_for_any_persona() {
        let plain = AskingChat {
            persona: None,
            held: false,
        };
        assert_eq!(
            decided(
                Asker::Chat(plain),
                Persona::None,
                &Covers::Covered,
                &alone()
            ),
            Decision::Start
        );
        assert_eq!(
            decided(
                Asker::Chat(plain),
                Persona::Defined("devops"),
                &Covers::NeedsGrant,
                &alone()
            ),
            Decision::NeedsGrant {
                from: None,
                to: "devops".to_owned(),
            }
        );
    }

    #[test]
    fn a_chat_holding_another_personas_grants_is_refused_and_told_to_have_its_own_allowed() {
        // #1362, D-1436-19: a chat handed off to `devops` from `qa` runs with qa's grants until
        // the person allows its own. A dispatch to "its own" persona would start a chat with
        // devops's hosts and vaults, which is the climb the hold exists to stop. It is refused
        // outright, never answered "needs a grant": a grant named devops to devops would read
        // as harmless and lift the hold for every chat like it.
        let held = AskingChat {
            persona: Some("devops"),
            held: true,
        };
        for to in [
            Persona::Defined("devops"),
            Persona::Defined("qa"),
            Persona::None,
        ] {
            // Whatever grant is in force for the pair its name suggests.
            for grant in [Covers::Covered, Covers::NeedsGrant] {
                let said = refused(decided(Asker::Chat(held), to, &grant, &alone()));
                assert_eq!(said, Refused::Held, "{to:?}");
                assert!(
                    said.say().contains("allow this chat its own grants"),
                    "{}",
                    said.say()
                );
            }
        }
        // The person dispatching from its tab is not held to it.
        assert_eq!(
            decided(
                Asker::Person(held),
                Persona::Defined("devops"),
                &Covers::NeedsGrant,
                &alone()
            ),
            Decision::Start
        );
    }

    #[test]
    fn no_persona_named_is_only_ever_a_chats_own() {
        // The app names the asking chat's own persona when none is named. A request that
        // names none for a chat that runs as one is not one a grant could name.
        assert_eq!(
            refused(decided(
                Asker::Chat(steward()),
                Persona::None,
                &Covers::Covered,
                &alone()
            )),
            Refused::NoPersonaNamed
        );
    }

    #[test]
    fn the_person_dispatches_to_any_persona_with_no_grant() {
        assert_eq!(
            decided(
                Asker::Person(steward()),
                Persona::Defined("devops"),
                &Covers::NeedsGrant,
                &alone()
            ),
            Decision::Start
        );
    }

    #[test]
    fn a_helper_sub_agent_is_refused_before_anything_else_is_asked() {
        // Even for a persona that is not there: the first question is who asks.
        let said = refused(decided(
            Asker::Helper,
            Persona::Unknown("nobody"),
            &Covers::Covered,
            &alone(),
        ));
        assert_eq!(said, Refused::Helper);
        assert!(said.say().contains("Return what you found to your chat"));
    }

    #[test]
    fn an_unknown_persona_and_a_draft_are_refused_and_never_ask_for_a_grant() {
        let ask = |asker, to| refused(decided(asker, to, &Covers::NeedsGrant, &alone()));
        let unknown = ask(Asker::Chat(steward()), Persona::Unknown("nobody"));
        assert_eq!(unknown, Refused::NoPersona("nobody".to_owned()));
        assert!(unknown.say().contains("purlis persona list"));
        let draft = ask(Asker::Chat(steward()), Persona::Draft("intern"));
        assert_eq!(draft, Refused::Draft("intern".to_owned()));
        assert!(draft.say().contains("draft: true"));
        // The person is refused the same: a grant is not what is missing.
        assert_eq!(
            ask(Asker::Person(steward()), Persona::Draft("intern")),
            Refused::Draft("intern".to_owned())
        );
    }

    #[test]
    fn a_chain_goes_as_deep_as_the_limit_and_no_deeper() {
        let at = |depth| Lineage { depth, ..alone() };
        let ask = |depth| {
            decided(
                Asker::Chat(steward()),
                Persona::Defined("steward"),
                &Covers::Covered,
                &at(depth),
            )
        };
        assert_eq!(ask(2), Decision::Start);
        assert_eq!(
            refused(ask(3)),
            Refused::Limit(dispatchlimits::Refused::TooDeep { limit: 3, depth: 3 })
        );
    }

    /// V100-58 (#1509): **the loop rule is no grant's to lift.** The decision is handed "the
    /// pair is covered", which is everything a grant can ever say to it: one pair's grant at
    /// any level, or one that names every persona. It refuses the same.
    #[test]
    fn a_persona_above_the_asking_chat_is_refused_whatever_is_granted() {
        // Room under every limit and a depth of 8, so nothing but the loop rule can refuse.
        let limits = dispatchlimits::in_force(
            &Table {
                project: Level::unset().with(Limit::Depth, 8),
                ..Table::default()
            },
            None,
            Some("devops"),
            Some("steward"),
            &Table::default(),
            &Level::unset(),
        );
        let devops = AskingChat {
            persona: Some("devops"),
            held: false,
        };
        let under = |above: &[&str]| Lineage {
            depth: u32::try_from(above.len()).expect("a depth"),
            chain: above.iter().map(|one| Some((*one).to_owned())).collect(),
            ..alone()
        };
        let ask = |asker, mode, grant: &Covers, lineage: &Lineage| {
            decide(&Request {
                asker,
                to: Persona::Defined("steward"),
                mode,
                grant,
                profile: None,
                limits: &limits,
                lineage,
            })
        };
        for above in [
            &["steward"][..],
            &["qa", "steward"],
            &["steward", "qa", "steward"],
        ] {
            for grant in [Covers::Covered, Covers::NeedsGrant] {
                // The person asking from the chat's tab needs no grant at all, and is refused
                // the same.
                for asker in [Asker::Chat(devops), Asker::Person(devops)] {
                    for mode in [Mode::Task, Mode::Handoff] {
                        assert_eq!(
                            ask(asker, mode, &grant, &under(above)),
                            Decision::Refused(Refused::Limit(dispatchlimits::Refused::Loop(
                                "steward".to_owned()
                            ))),
                            "{above:?} {grant:?} {asker:?} {mode:?}"
                        );
                    }
                }
            }
        }
        // The same ask from a chain steward is not in starts, so the refusal above is the
        // loop rule's and nothing else's.
        assert_eq!(
            ask(
                Asker::Chat(devops),
                Mode::Task,
                &Covers::Covered,
                &under(&["qa", "reviewer"])
            ),
            Decision::Start
        );
    }

    /// And it is not the depth limit: one dispatch below the chat the person started, with a
    /// depth of 8 allowed, a covered dispatch back up is refused as a loop.
    #[test]
    fn the_loop_rule_refuses_where_the_depth_limit_has_room_and_is_said_as_itself() {
        let limits = dispatchlimits::in_force(
            &Table {
                project: Level::unset().with(Limit::Depth, 8),
                ..Table::default()
            },
            None,
            Some("devops"),
            Some("steward"),
            &Table::default(),
            &Level::unset(),
        );
        let lineage = Lineage {
            depth: 1,
            chain: vec![Some("steward".to_owned())],
            ..alone()
        };
        let said = refused(decide(&Request {
            asker: Asker::Chat(AskingChat {
                persona: Some("devops"),
                held: false,
            }),
            to: Persona::Defined("steward"),
            mode: Mode::Task,
            grant: &Covers::Covered,
            profile: None,
            limits: &limits,
            lineage: &lineage,
        }));
        assert_eq!(
            said,
            Refused::Limit(dispatchlimits::Refused::Loop("steward".to_owned()))
        );
        assert_eq!(
            said.say(),
            "persona 'steward' is already in this chat's own chain of dispatches, and a \
             persona is never dispatched to from below itself. Send it what you found in your \
             report instead."
        );
    }

    /// The same through the join, from the app's records and the grants in force: a grant of
    /// the pair at every level there is does not let a chat dispatch back up its own chain.
    #[test]
    fn a_grant_at_every_level_does_not_let_a_chat_dispatch_back_up_its_chain() {
        use crate::dispatchgrant::{ChatPair, Pair as Granted};
        // 1 (steward) ── 2 (devops, task)
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let open = [(1, &one), (2, &two)];
        let pair = Granted::new("devops", "steward").expect("a pair");
        let granted = InForce {
            chat: vec![ChatPair {
                asking: Some("devops".to_owned()),
                target: "steward".to_owned(),
            }],
            you: vec![pair.clone()],
            project: vec![pair],
            // And "any persona" at both of its levels (#1503, on this train): the wildcard
            // the ticket's criterion is about, now that there is one.
            you_any: vec!["devops".to_owned()],
            project_any: vec!["devops".to_owned()],
            ..InForce::default()
        };
        for by in [By::Chat, By::Person] {
            let said = asked_in(
                "[dispatch]\ndepth = 8\n",
                2,
                &open,
                Some("steward"),
                &granted,
                by,
            );
            assert_eq!(
                said.decision,
                Decision::Refused(Refused::Limit(dispatchlimits::Refused::Loop(
                    "steward".to_owned()
                ))),
                "{by:?}"
            );
        }
        // The grant is real: the same chat, with nobody above it, is started on it.
        let alone = chat(Some("devops"));
        assert_eq!(
            asked_in(
                "[dispatch]\ndepth = 8\n",
                2,
                &[(2, &alone)],
                Some("steward"),
                &granted,
                By::Chat
            )
            .decision,
            Decision::Start
        );
    }

    #[test]
    fn an_asking_chat_has_at_most_its_limit_running_and_a_lineage_holds_at_most_its_own() {
        let ask = |lineage: Lineage| {
            decided(
                Asker::Chat(steward()),
                Persona::Defined("steward"),
                &Covers::Covered,
                &lineage,
            )
        };
        assert_eq!(
            ask(Lineage {
                running: 5,
                lineage: 15,
                ..alone()
            }),
            Decision::Start
        );
        assert_eq!(
            refused(ask(Lineage {
                running: 6,
                ..alone()
            })),
            Refused::Limit(dispatchlimits::Refused::TooManyRunning {
                limit: 6,
                running: 6
            })
        );
        assert_eq!(
            refused(ask(Lineage {
                lineage: 16,
                ..alone()
            })),
            Refused::Limit(dispatchlimits::Refused::LineageFull {
                limit: 16,
                lineage: 16
            })
        );
    }

    #[test]
    fn the_limits_hold_for_the_person_too_and_are_said_before_a_grant_is_asked_for() {
        // Asking the person for a grant that would start nothing wastes their yes.
        let full = Lineage {
            running: 6,
            ..alone()
        };
        for asker in [Asker::Person(steward()), Asker::Chat(steward())] {
            assert!(matches!(
                refused(decided(
                    asker,
                    Persona::Defined("devops"),
                    &Covers::NeedsGrant,
                    &full
                )),
                Refused::Limit(dispatchlimits::Refused::TooManyRunning { .. })
            ));
        }
    }

    #[test]
    fn a_policy_lock_is_said_before_a_limit_and_a_refused_policy_file_before_either() {
        let locked = Covers::Locked("Policy forbids one chat dispatching to another.".to_owned());
        let full = Lineage {
            running: 6,
            ..alone()
        };
        let own = Persona::Defined("steward");
        assert!(matches!(
            refused(decided(Asker::Chat(steward()), own, &locked, &full)),
            Refused::Locked(_)
        ));
        // The file was refused: nothing in it was read, so nothing is said to be locked or
        // set. Dispatch is off, and the sentence says the file is why.
        let off = dispatchlimits::in_force(
            &Table::default(),
            None,
            Some("steward"),
            Some("steward"),
            &Table::default(),
            &dispatchlimits::ceiling_when_refused(),
        );
        let said = refused(decide(&Request {
            asker: Asker::Chat(steward()),
            to: own,
            mode: Mode::Task,
            grant: &locked,
            profile: None,
            limits: &off,
            lineage: &alone(),
        }));
        assert_eq!(
            said,
            Refused::Limit(dispatchlimits::Refused::Off {
                limit: Limit::RunningPerChat,
                by: Source::PolicyRefused,
                target: Some("steward".to_owned()),
            })
        );
        assert_eq!(
            said.say(),
            "dispatch is off on this machine: its policy file is refused, so purlis cannot \
             tell what an administrator allows. Only an administrator can fix it."
        );
    }

    #[test]
    fn no_profile_to_start_on_is_refused_in_the_profile_s_own_sentence() {
        use crate::personaprofile;
        let limits = defaults(Some("steward"), Some("devops"));
        let ask = |why: &personaprofile::Refused, lineage: &Lineage| {
            refused(decide(&Request {
                asker: Asker::Chat(steward()),
                to: Persona::Defined("devops"),
                mode: Mode::Task,
                grant: &Covers::NeedsGrant,
                profile: Some(why),
                limits: &limits,
                lineage,
            }))
        };
        // A chat on no profile, dispatching to a persona that names none (D-1445-7: one that
        // names a profile starts on it, and is not refused here).
        let none = personaprofile::Refused::NoProfile;
        assert_eq!(ask(&none, &alone()), Refused::Profile(none.clone()));
        assert_eq!(ask(&none, &alone()).say(), none.say());
        // A profile the dispatch named that the project does not offer: said before a limit,
        // and before the person is asked for a grant.
        let unoffered = personaprofile::Refused::NotOffered {
            profile: "prod".to_owned(),
            by: personaprofile::Who::Asker,
            persona: None,
        };
        let full = Lineage {
            running: 6,
            ..alone()
        };
        assert_eq!(ask(&unoffered, &full), Refused::Profile(unoffered.clone()));
    }

    #[test]
    fn every_refusal_is_one_line_that_says_what_to_do() {
        for refusal in [
            Refused::Helper,
            Refused::Held,
            Refused::NoPersonaNamed,
            Refused::NoPersona("x".to_owned()),
            Refused::Draft("x".to_owned()),
            Refused::Locked("Policy forbids one chat dispatching to another.".to_owned()),
            Refused::Profile(crate::personaprofile::Refused::NoProfile),
            Refused::Profile(crate::personaprofile::Refused::NotListed {
                profile: "x".to_owned(),
                by: crate::personaprofile::Who::Asker,
                persona: "y".to_owned(),
                listed: vec!["z".to_owned()],
            }),
            Refused::Limit(dispatchlimits::Refused::Loop("x".to_owned())),
            Refused::Limit(dispatchlimits::Refused::TooDeep { limit: 3, depth: 3 }),
            Refused::Limit(dispatchlimits::Refused::TooManyRunning {
                limit: 6,
                running: 6,
            }),
            Refused::Limit(dispatchlimits::Refused::LineageFull {
                limit: 16,
                lineage: 16,
            }),
        ] {
            let said = refusal.say();
            assert!(!said.contains('\n'), "{said}");
            assert!(said.matches(". ").count() >= 1, "two sentences: {said}");
        }
    }

    // ----- what the persona chat starts with ------------------------------------------------

    #[test]
    fn a_persona_chat_takes_the_asking_chats_folder_and_nothing_else_of_it() {
        // The asking chat holds another persona's grants, ran its last run without the
        // sandbox, draws its footer and is pinned. None of it is the new chat's.
        let asking = Chat {
            cwd: Some("/plane/workspaces/alpha/svc".into()),
            held: Some(HeldGrants {
                persona: Some("qa".to_owned()),
            }),
            unsandboxed: true,
            show_footer: true,
            pinned: true,
            args: vec!["--dangerously-skip-permissions".to_owned()],
            resume: crate::harness::SessionId::new("0b0f2b7e-6f0f-4b6e-9d57-0d0c5a3c2b1a").ok(),
            ..chat(Some("steward"))
        };

        let start = start_for(
            &asking,
            Some(crate::dispatchgrant::grants_for_a_dispatched_chat("devops")),
            "codex-work".to_owned(),
            "7".to_owned(),
        );

        assert_eq!(
            start,
            crate::start::Start {
                // The profile chosen for it, which need not be the asking chat's.
                profile: Some("codex-work".to_owned()),
                persona: Some("devops".to_owned()),
                name: "7".to_owned(),
                cwd: Some("/plane/workspaces/alpha/svc".into()),
                resume: None,
                show_footer: false,
                resuming: None,
                without_sandbox: None,
                held: None,
                grants: crate::sandbox::grant::Grants::default(),
            }
        );
        // A chat on no persona dispatching to none starts one on none, holding nothing.
        let plain = start_for(&asking, None, "work".to_owned(), "8".to_owned());
        assert_eq!((plain.persona, plain.held), (None, None));
    }

    const DEVOPS_AND_QA: &str = "[sandbox]\nmode = \"on\"\negress = []\n\
        [sandbox.personas.devops]\nhosts = [\"10.100.39.145:6443\"]\n\
        [sandbox.personas.qa]\nhosts = [\"qa.example\"]\n";

    /// The hosts the sandbox compiled for `start` reaches, in a project whose file is `text`.
    fn reached(text: &str, start: &crate::start::Start) -> Vec<String> {
        use crate::sandbox::{Compiled, Machine, Os, Plane};
        let plane = Plane::of(Some(text));
        let policy = plane.said().policy.expect("the sandbox is on");
        let root = tempfile::tempdir().expect("a project");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(std::path::PathBuf::from("/home/op")),
            os: Os::MacOs,
        };
        // The persona a start compiles its grants for, exactly as `start::ready` picks it.
        let persona =
            crate::start::grants_persona(start.held.as_ref(), start.persona.as_deref(), || None);
        Compiled::granted(
            &policy,
            &plane,
            root.path(),
            &machine,
            persona.as_deref(),
            &start.grants,
        )
        .hosts
    }

    fn start_as(asking: &Chat, persona: &str) -> crate::start::Start {
        start_for(
            asking,
            Some(crate::dispatchgrant::grants_for_a_dispatched_chat(persona)),
            "work".to_owned(),
            "7".to_owned(),
        )
    }

    #[test]
    fn the_persona_chats_compiled_sandbox_is_the_projects_for_its_persona_and_not_the_askers() {
        // The asking chat runs as qa, and the person let it reach one more host from a block's
        // Notice. What a chat it dispatches reaches is the project's answer for that chat's
        // own persona: not qa's host, and not the one the person allowed the asking chat.
        let asking = chat(Some("qa"));
        let mut askers_own = crate::start::Start {
            persona: Some("qa".to_owned()),
            ..Default::default()
        };
        askers_own
            .grants
            .hosts
            .push(crate::sandbox::hosts::Host::parse("allowed-once.example").expect("a host"));
        assert_eq!(
            reached(DEVOPS_AND_QA, &askers_own),
            ["allowed-once.example", "qa.example"],
            "what the asking chat itself reaches"
        );

        assert_eq!(
            reached(DEVOPS_AND_QA, &start_as(&asking, "qa")),
            ["qa.example"]
        );
        assert_eq!(
            reached(DEVOPS_AND_QA, &start_as(&asking, "devops")),
            ["10.100.39.145:6443"]
        );
    }

    #[test]
    fn a_chat_that_opted_out_of_the_sandbox_dispatches_a_chat_that_did_not() {
        // The opt-out is the picker's, for one chat. The start a dispatch makes carries none,
        // so the sandbox decision is the project's: sandboxed, or not started.
        let asking = Chat {
            unsandboxed: true,
            ..chat(Some("qa"))
        };
        assert_eq!(start_as(&asking, "qa").without_sandbox, None);
    }

    #[test]
    fn a_persona_is_read_from_the_projects_definition() {
        let root = a_project("");
        assert_eq!(
            persona_in(root.path(), "devops"),
            Persona::Defined("devops")
        );
        assert_eq!(persona_in(root.path(), "intern"), Persona::Draft("intern"));
        assert_eq!(
            persona_in(root.path(), "nobody"),
            Persona::Unknown("nobody")
        );
        // A name that cannot be a persona's is no persona, wherever it would point.
        assert_eq!(
            persona_in(root.path(), "../devops"),
            Persona::Unknown("../devops")
        );
    }

    #[test]
    fn a_chat_on_no_persona_is_in_a_chain_as_the_default_persona() {
        let one = chat(None);
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let own = pair(Some("devops"), Some("devops"));
        assert_eq!(
            lineage_of(2, &[(1, &one), (2, &two)], Some("steward"), &|_| true, &own).chain,
            vec![Some("steward".to_owned())]
        );
        assert_eq!(
            lineage_of(2, &[(1, &one), (2, &two)], None, &|_| true, &own).chain,
            vec![None]
        );
    }

    #[test]
    fn a_chat_whose_asker_has_closed_keeps_the_depth_its_record_holds() {
        // Chat 2 closed. Chat 4's own record says it is two dispatches down, and that is
        // what counts: a chain never looks shallower because a chat above it went away.
        let four = dispatched(2, 2, Mode::Task, Owed::Due, Some("devops"));
        let seen = seen(4, &[(4, &four)]);
        assert_eq!(seen.depth, 2);
        // A record written before the chain was kept: who was above is not known, and never
        // read as nobody (#1521).
        assert_eq!(seen.chain, Vec::<Option<String>>::new());
        assert!(seen.chain_unread);
        assert_eq!(seen.lineage, 1);
    }

    #[test]
    fn records_that_name_each_other_are_walked_once() {
        // The record is a file anything running as the person can write. Two chats that each
        // say the other dispatched it are a loop no dispatch made, and reading it ends, with
        // the chain above read as one purlis cannot read whole.
        let one = dispatched(2, 1, Mode::Task, Owed::Due, Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let seen = seen(1, &[(1, &one), (2, &two)]);
        assert_eq!(seen.chain, vec![Some("devops".to_owned())]);
        assert!(seen.chain_unread);
        assert_eq!(seen.lineage, 2);
    }

    // ----- the chain, from the dispatch record (#1521) --------------------------------------

    /// [`dispatched`], keeping the personas above it as the app writes them.
    fn kept(by: u32, above: &[Option<&str>], persona: Option<&str>) -> Chat {
        let depth = u32::try_from(above.len()).expect("a depth");
        let mut chat = dispatched(by, depth, Mode::Task, Owed::Due, persona);
        if let Some(from) = chat.from.as_mut() {
            from.above = Some(above.iter().map(|one| one.map(str::to_owned)).collect());
        }
        chat
    }

    fn grant(pairs: &[(&str, &str)]) -> InForce {
        InForce {
            you: pairs
                .iter()
                .map(|(from, to)| crate::dispatchgrant::Pair::new(from, to).unwrap())
                .collect(),
            ..Default::default()
        }
    }

    fn loop_to(name: &str) -> Decision {
        Decision::Refused(Refused::Limit(dispatchlimits::Refused::Loop(
            name.to_owned(),
        )))
    }

    #[test]
    fn a_persona_above_a_chat_that_closed_is_still_refused_by_the_loop_rule() {
        // steward (1) dispatched devops (2), and chat 1 has closed. Devops asks for steward:
        // A to B to A, with the first A gone. Its own record still says who asked it.
        let two = kept(1, &[Some("steward")], Some("devops"));
        let open = [(2, &two)];
        let grants = grant(&[("devops", "steward")]);
        let said = asked_in("", 2, &open, Some("steward"), &grants, By::Chat);
        assert_eq!(said.decision, loop_to("steward"));
        // The person's own ask from its tab is held to the loop rule too.
        assert_eq!(
            asked_in("", 2, &open, Some("steward"), &grants, By::Person).decision,
            loop_to("steward")
        );
        // Three down, with both chats in the middle closed, finished or cleared.
        let three = kept(2, &[Some("devops"), Some("steward")], Some("qa"));
        let open = [(3, &three)];
        let grants = grant(&[("qa", "steward"), ("qa", "devops")]);
        for to in ["steward", "devops"] {
            assert_eq!(
                asked_in(
                    "[dispatch]\ndepth = 8\n",
                    3,
                    &open,
                    Some(to),
                    &grants,
                    By::Chat
                )
                .decision,
                loop_to(to)
            );
        }
        // A persona not above it is not refused by the rule.
        let grants = grant(&[("qa", "ops")]);
        assert_eq!(
            asked_in(
                "[dispatch]\ndepth = 8\n",
                3,
                &open,
                Some("ops"),
                &grants,
                By::Chat
            )
            .decision,
            Decision::Start
        );
    }

    #[test]
    fn a_chat_that_left_before_its_ask_is_decided_starts_nothing_and_keeps_no_chain() {
        // #1521: devops (2), under steward, asked; by the time the app decides, under its lock,
        // chat 2 has ended. Read from the chats open then, it would have nobody above it, and
        // a shorter chain would be written into the chat it starts. It is refused instead.
        let two = kept(1, &[Some("steward")], Some("devops"));
        let three = kept(1, &[Some("steward")], Some("qa"));
        let grants = grant(&[("devops", "steward"), ("devops", "qa")]);
        for to in ["steward", "qa"] {
            for by in [By::Chat, By::Person] {
                let said = asked_in("", 2, &[(3, &three)], Some(to), &grants, by);
                assert_eq!(said.decision, Decision::Refused(Refused::NotOpen(2)));
                assert_eq!((said.depth, said.above.clone()), (0, None));
                assert_eq!(refusal(&said), "chat 2 is not one this app has open");
            }
        }
        // While it is open, the same ask is decided over its own record.
        assert_eq!(
            asked_in("", 2, &[(2, &two)], Some("steward"), &grants, By::Chat).decision,
            loop_to("steward")
        );
    }

    #[test]
    fn the_record_s_chain_is_read_whatever_the_chats_still_open_say() {
        // The chat above is open, but under a number that is now another chat's: the walk
        // would read reviewer above it. What the asking chat's own record keeps is the chain.
        let one = started(Some("reviewer"), ROOT_ID);
        let two = kept(1, &[Some("steward")], Some("devops"));
        let seen = seen(2, &[(1, &one), (2, &two)]);
        assert_eq!(seen.chain, vec![Some("steward".to_owned())]);
        assert!(!seen.chain_unread);
    }

    #[test]
    fn the_person_s_never_for_a_chat_above_holds_when_a_chat_in_the_middle_closed() {
        // steward (1) to devops (2) to qa (3); 1 and 2 have closed. The person said never to
        // steward chats dispatching to ops: it holds for qa, below steward.
        let three = kept(2, &[Some("devops"), Some("steward")], Some("qa"));
        let open = [(3, &three)];
        let grants = InForce {
            never: vec![("steward".to_owned(), "ops".to_owned())],
            ..grant(&[("qa", "ops")])
        };
        let said = asked_in(
            "[dispatch]\ndepth = 8\n",
            3,
            &open,
            Some("ops"),
            &grants,
            By::Chat,
        );
        assert_eq!(
            said.decision,
            Decision::Refused(Refused::Never(crate::dispatchgrant::never_above_said(
                "steward", "ops"
            )))
        );
    }

    #[test]
    fn a_dispatch_keeps_the_personas_above_the_chat_it_starts() {
        // The person's chat (1, steward) asks for devops: steward is above the new chat.
        let one = started(Some("steward"), ROOT_ID);
        let grants = grant(&[("steward", "devops"), ("devops", "qa")]);
        let first = asked_in("", 1, &[(1, &one)], Some("devops"), &grants, By::Chat);
        assert_eq!(
            (first.depth, first.above),
            (1, Some(vec![Some("steward".to_owned())]))
        );
        // That chat (2) asks for qa with chat 1 closed: its own record, then itself.
        let two = kept(1, &[Some("steward")], Some("devops"));
        let second = asked_in("", 2, &[(2, &two)], Some("qa"), &grants, By::Chat);
        assert_eq!(
            (second.depth, second.above.clone()),
            (
                2,
                Some(vec![Some("devops".to_owned()), Some("steward".to_owned())])
            )
        );
        // The person asking from a tab: the chain is the tab's chat's, the same.
        let by_person = asked_in("", 2, &[(2, &two)], Some("qa"), &grants, By::Person);
        assert_eq!(by_person.above, second.above);
    }

    #[test]
    fn a_finished_task_reopened_as_an_ordinary_chat_starts_a_new_chain() {
        // A reopened task names no asking chat: nothing is above it, and what it dispatches
        // has only it above.
        let reopened = chat(Some("devops"));
        let grants = grant(&[("devops", "steward")]);
        let said = asked_in("", 5, &[(5, &reopened)], Some("steward"), &grants, By::Chat);
        assert_eq!(said.decision, Decision::Start);
        assert_eq!(
            (said.depth, said.above),
            (1, Some(vec![Some("devops".to_owned())]))
        );
    }

    #[test]
    fn an_older_record_is_read_from_the_chats_still_open_as_before() {
        // Written before the chain was kept, with every chat above it still open: the walk
        // reads it whole, and the new chat keeps it.
        let one = started(Some("steward"), ROOT_ID);
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let open = [(1, &one), (2, &two)];
        let grants = grant(&[("devops", "steward"), ("devops", "qa")]);
        assert_eq!(
            asked_in("", 2, &open, Some("steward"), &grants, By::Chat).decision,
            loop_to("steward")
        );
        let said = asked_in("", 2, &open, Some("qa"), &grants, By::Chat);
        assert_eq!(said.decision, Decision::Start);
        assert_eq!(
            said.above,
            Some(vec![Some("devops".to_owned()), Some("steward".to_owned())])
        );
        // The walk ends at the first record above that keeps its own chain.
        let three = dispatched(2, 2, Mode::Task, Owed::Due, Some("qa"));
        let two = kept(1, &[Some("steward")], Some("devops"));
        let seen = seen(3, &[(2, &two), (3, &three)]);
        assert_eq!(
            seen.chain,
            vec![Some("devops".to_owned()), Some("steward".to_owned())]
        );
        assert!(!seen.chain_unread);
    }

    #[test]
    fn an_older_record_whose_chain_has_a_closed_chat_is_refused_any_other_persona_and_told_why() {
        // Written before the chain was kept, and the chat that asked it has closed: who was
        // above is not known, so any persona but its own may have been, and is refused.
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let open = [(2, &two)];
        let grants = grant(&[("devops", "steward"), ("devops", "qa")]);
        for to in ["steward", "qa"] {
            let said = asked_in("", 2, &open, Some(to), &grants, By::Chat);
            let unread = dispatchlimits::Refused::ChainUnread(to.to_owned());
            assert_eq!(
                said.decision,
                Decision::Refused(Refused::Limit(unread.clone()))
            );
            assert!(
                unread
                    .say()
                    .starts_with("this chat's chain began under an older version"),
                "{}",
                unread.say()
            );
            // The person's own ask from its tab is held to it too.
            assert_eq!(
                asked_in("", 2, &open, Some(to), &grants, By::Person).decision,
                said.decision
            );
        }
        // Its own persona is not above it, so it may split its own work, and what it starts
        // keeps no chain: its asks are held the same way.
        let own = asked_in("[dispatch]\ndepth = 8\n", 2, &open, None, &grants, By::Chat);
        assert_eq!(own.decision, Decision::Start);
        assert_eq!(own.above, None);
    }

    #[test]
    fn an_older_record_whose_chain_has_a_closed_chat_holds_every_never_to_the_target() {
        // Its own persona: a never from any persona to it may be one from above. Refused for
        // the chat, and not for the person, whom no never holds.
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let open = [(2, &two)];
        let grants = InForce {
            never: vec![("steward".to_owned(), "devops".to_owned())],
            ..Default::default()
        };
        let depth = "[dispatch]\ndepth = 8\n";
        assert_eq!(
            asked_in(depth, 2, &open, Some("devops"), &grants, By::Chat).decision,
            Decision::Refused(Refused::Limit(dispatchlimits::Refused::ChainUnread(
                "devops".to_owned()
            )))
        );
        assert_eq!(
            asked_in(depth, 2, &open, Some("devops"), &grants, By::Person).decision,
            Decision::Start
        );
        // No never to it: its own work splits as before.
        assert_eq!(
            asked_in(
                depth,
                2,
                &open,
                Some("devops"),
                &InForce::default(),
                By::Chat
            )
            .decision,
            Decision::Start
        );
    }

    #[test]
    fn a_persona_chat_at_a_permission_prompt_stands_waiting_on_the_operator() {
        // #1443: what its asking chat's list says of it.
        assert_eq!(
            standing(Owed::Due, false, true),
            Standing::WaitingOnOperator
        );
        assert_eq!(Standing::WaitingOnOperator.word(), "waiting on the person");
        assert_eq!(standing(Owed::Due, false, false), Standing::Running);
        assert_eq!(Standing::Running.word(), "running");
    }

    #[test]
    fn a_report_settles_where_a_persona_chat_stands_whatever_it_does_afterwards() {
        for (ended, waits) in [(false, false), (false, true), (true, false), (true, true)] {
            assert_eq!(standing(Owed::Sent, ended, waits), Standing::Reported);
            assert_eq!(standing(Owed::Failed, ended, waits), Standing::Ended);
        }
        assert_eq!(Standing::Reported.word(), "reported");
        // A program that is gone is not waiting on anybody.
        assert_eq!(standing(Owed::Due, true, true), Standing::Ended);
        assert_eq!(Standing::Ended.word(), "ended without a report");
    }

    #[test]
    fn only_a_chat_still_to_report_is_in_flight() {
        assert!(Standing::Running.in_flight());
        assert!(Standing::WaitingOnOperator.in_flight());
        assert!(!Standing::Reported.in_flight());
        assert!(!Standing::Ended.in_flight());
    }

    #[test]
    fn a_mode_is_read_back_from_its_word_and_anything_else_is_a_handoff() {
        assert_eq!(Mode::of(Mode::Task.word()), Mode::Task);
        assert_eq!(Mode::of(Mode::Handoff.word()), Mode::Handoff);
        assert_eq!(Mode::of(""), Mode::Handoff);
    }
}
