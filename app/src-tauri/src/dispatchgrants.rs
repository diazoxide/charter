//! **The app's half of a dispatch grant** (#1437, spec #1434): the dispatch a chat asked for
//! that no grant covers, held here until the person answers its Notice; the grants made for one
//! chat; and the commands the Notice and Settings press.
//!
//! # The contract the dispatch core builds on
//!
//! Two entry points, one answer type ([`Requested`]):
//!
//! - [`request_dispatch_grant`]`(held, session, target, brief)`, for a chat a person is at.
//!   Covered: start it, with what [`Requested::Covered`] carries. Needs a grant: nothing
//!   starts, the Notice is raised on the asking chat's tab, and the dispatch is held here.
//!   Locked: refused with the policy's sentence, and the Notice says the same. Refused: a
//!   sentence for the chat.
//! - [`request_dispatch_grant_or_refuse`], for **a chat nobody is at** (an unattended chat,
//!   spec decision 20): the same, but an uncovered or locked dispatch is a plain refusal.
//!   Nothing is held and no Notice is raised, so nobody can allow it later by accident.
//!
//! What the caller must hold to, because this module cannot:
//!
//! - **`session` is the sender's**, taken from the chat the line's token is bound to, and a
//!   helper sub-agent is refused before either entry point is called. Both trust `session`.
//! - **The asking persona is the one the chat runs with**
//!   ([`purlis_core::start::runs_with`]), never the one its record or the request names. A
//!   chat that runs on another chat's grants until the person allows its own (a handoff's or a
//!   Resume's hold) dispatches to no one: its request is refused with a sentence, never held.
//! - **Start only what an answer carries.** Asked twice for one target while the first waits,
//!   the second request gets the first's number and its brief is dropped: the person saw one
//!   brief, so only [`Answered::pending`] starts, never a second brief queued beside it.
//! - **[`Pending::brief`] is the brief as shown** (escaped, and cut past the bound), for the
//!   Notice. The caller keeps the brief it will send.
//! - **A held dispatch whose chat closes is dropped**, and the [`Store::answers_with`] listener
//!   is not told: there is no chat left to tell.
//! - On Allow, and when the person allows a teammate's committed pair on the project's Notice,
//!   every held dispatch the grant covers is handed to the listener, which starts it.
//!
//! # What a chat cannot do
//!
//! **Nothing a chat sends creates, widens or revokes a grant.** A grant is made by
//! [`allow_dispatch`] or [`acknowledge_dispatch_grants`], window commands a person's press
//! sends, and by nothing on the hook socket: a request carries a target and a brief, and
//! neither is read for anything but the Notice. **The brief is a chat's text**: it is shown
//! inert and capped ([`purlis_core::dispatchgrant::shown_brief`]), apart from purlis's own
//! words. Every Allow and every Revoke is audited before it takes effect, and one that cannot
//! be written is found before it is audited.
//!
//! **Where the sandbox is what holds that.** "Me on this machine" and "everyone in this
//! project" are files. A sandboxed chat is denied writing both. A chat in a project with the
//! sandbox off, or one a person started without it, runs as the person and can write either:
//! nothing here is a boundary against that chat (D-1437-R2).
//!
//! # No, for this chat and for good (#1503)
//!
//! **Keep blocked holds for the chat's life** ([`Store::keep_blocked`]): the same chat asking
//! across the same pair again is refused at once with the person's no, and the person is not
//! asked again. It is the app's, in memory, and ends with the chat as its grants do; a new chat
//! is asked. It stands in for the question only: a grant the person makes afterwards covers
//! the chat like any other.
//!
//! **Never for this pair** ([`Store::never`]) is the person's on this machine
//! (`app/dispatch-never.json`), and beats every grant: no chat of that persona is asked or
//! allowed for that target until the person lifts it in Settings ([`lift_dispatch_never`]). It
//! holds for a chat with a chat of that persona above it too, which the dispatch decision
//! reads before this store is asked. **Where that record does not read, no grant counts**: the
//! person is asked, told why, and their Allow starts the one dispatch they read.
//!
//! **Any persona** ([`allow_dispatch_to_any`]) is granted from Settings and from nowhere else.
//! No answer to a Notice makes one: [`Store::allow`] keeps the one pair the held dispatch
//! names, whose target is a persona's name, and the project's Notice accepts pairs only.
//!
//! **A "this chat" grant ends with the chat** ([`Store::chat_closed`]), as its sandbox grants
//! do, and stays through a restart, which starts the new run before the old one ends
//! (D-1437-R3).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use purlis_core::dispatchgrant::{self, ChatPair, Covers, Dispatched, InForce, Pair};
use purlis_core::sandbox;
use purlis_core::sandbox::grant::Level;

use crate::planes::{PlaneId, Planes};
use crate::sandboxing::GrantLevel;

/// The event the window is sent when a dispatch needs the person: its payload is a
/// [`DispatchPending`].
pub const NEEDED: &str = "dispatch-grant-needed";

/// The most dispatches one chat may have waiting on the person at once: a chat that asks in a
/// loop is refused, and told so, long before it fills the window.
pub const MOST_WAITING_PER_CHAT: usize = 5;

/// The asking chat, as this app records it: never as a request says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    /// The app's number for the chat.
    pub session: u32,
    /// Its id, which a grant for this chat is kept under; none for a chat with no id yet.
    pub id: Option<String>,
    /// Its name as its tab shows it.
    pub name: String,
    /// The persona whose authority it runs with ([`purlis_core::start::runs_with`]): the
    /// grants a handoff or a Resume left it holding, else its own, else the one a new chat
    /// here adopts. None for a chat on no persona.
    pub persona: Option<String>,
    /// Whether it runs on another chat's grants until the person allows its own on its tab
    /// (D-1362-5, D-1362-6). Such a chat dispatches to no one.
    pub held: bool,
}

/// The chat `chat` records, as an asking chat: session `session`, shown as `name`, in the
/// project at `root`.
pub fn asking_from(
    chat: &purlis_core::reopen::Chat,
    session: u32,
    name: String,
    root: &Path,
) -> Asking {
    Asking {
        session,
        id: chat.identity.id.clone(),
        name,
        persona: purlis_core::start::runs_with(chat, root),
        held: chat.held.is_some(),
    }
}

/// Chat `session` as `chats` records it, in the project at `root`; none for a chat this app
/// does not have open.
pub fn asking_of(chats: &crate::chats::Chats, root: &Path, session: u32) -> Option<Asking> {
    let chat = chats.recorded_chat(session)?;
    let name = chats
        .shown_name(session)
        .unwrap_or_else(|| chat.name.clone());
    Some(asking_from(&chat, session, name, root))
}

/// What a request is answered where nothing covers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uncovered {
    /// Hold it and raise the Notice: a person is at the chat.
    AskThePerson,
    /// Refuse it, hold nothing and raise nothing: nobody is at the chat.
    Refuse,
}

/// What a chat that runs on another chat's grants is told when it asks to dispatch.
pub const HELD_DISPATCHES_TO_NO_ONE: &str = "this chat runs on another chat's grants until the person allows its own on its tab, so it \
     dispatches to no one yet. Ask the person to press Allow on this chat's tab.";

/// A dispatch held until the person answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub id: u32,
    pub asking: Asking,
    pub target: String,
    /// The first brief, as the chat sent it, up to what the Notice shows.
    pub brief: dispatchgrant::ShownBrief,
    /// The policy's sentence, where policy locks it: it was refused, and the Notice only says
    /// so.
    pub locked: Option<String>,
    /// When it was asked, in seconds since 1970.
    pub at: u64,
}

/// One grant a person made for one chat, as Settings lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChatMade {
    pair: ChatPair,
    at: u64,
    /// The chat's name as its tab showed it then.
    chat: String,
}

/// What the person answered a held dispatch with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    pub pending: Pending,
    /// What the persona chat starts with, where the dispatch is now covered; `None` where the
    /// person kept it blocked.
    pub allowed: Option<Dispatched>,
}

/// Told each answer: the dispatch core registers it, and starts the chat an Allow covered or
/// tells the asking chat the person kept it blocked.
pub type Answers = Arc<dyn Fn(&Answered) + Send + Sync + 'static>;

/// Told each dispatch that needs the person, for the window.
pub type Teller = Arc<dyn Fn(DispatchPending) + Send + Sync + 'static>;

static TELL: OnceLock<Teller> = OnceLock::new();

/// Sends every dispatch that needs the person to `tell` from now on: the window, once, as the
/// app starts.
pub fn telling(tell: Teller) {
    let _ = TELL.set(tell);
}

/// What one project holds of dispatch grants: the dispatches waiting on the person, and the
/// grants made for one chat. **In memory only**: both end with the app, and a chat's with it.
#[derive(Default)]
pub struct Store {
    pending: Mutex<Vec<Pending>>,
    /// By chat id.
    chat: Mutex<HashMap<String, Vec<ChatMade>>>,
    next: AtomicU32,
    answers: Mutex<Option<Answers>>,
    /// The pairs the person kept blocked on a chat's tab, for that chat's life (#1503).
    kept_blocked: Mutex<HashMap<Whose, Vec<ChatPair>>>,
    /// The dispatches the person allowed while the record of nevers did not read, each for one
    /// start: no grant counts then, so their answer starts the one dispatch they read and no
    /// other ([`Store::allow`]).
    once: Mutex<Vec<(Whose, ChatPair)>>,
}

/// The chat a kept-blocked pair is remembered for: by its id, which a restart keeps, or by
/// its session where it has no id yet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Whose {
    Id(String),
    Session(u32),
}

impl Whose {
    fn of(asking: &Asking) -> Self {
        match &asking.id {
            Some(id) => Self::Id(id.clone()),
            None => Self::Session(asking.session),
        }
    }
}

/// What a chat is told when it asks again across a pair the person kept blocked on its tab.
pub fn kept_blocked_said(target: &str) -> String {
    let target = purlis_core::shown::short(target);
    format!(
        "the person answered Keep blocked when this chat asked to dispatch to {target}, so \
         nothing was started and they are not asked again in this chat. Do not dispatch to \
         {target} from this chat again. Do this work without {target}, or tell the person it \
         is waiting."
    )
}

fn lock<T>(held: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    held.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What [`request_dispatch_grant`] answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requested {
    /// Start it now, with these.
    Covered(Dispatched),
    /// Nothing starts. The Notice is raised and the dispatch is held under this number; an
    /// Allow hands it to [`Store::answers_with`]'s listener.
    NeedsGrant { pending: u32 },
    /// Policy locks it: the policy's sentence, which names who set it. Nothing is held to
    /// allow; the Notice says the same to the person.
    Locked(String),
    /// Not a dispatch this app can ask about, and why.
    Refused(String),
}

/// Writes the audit of a grant or revoke, and answers whether it was written.
pub type Audit<'a> = &'a dyn Fn(Option<u32>, &dispatchgrant::Audited<'_>) -> Result<(), String>;

/// What the store is asked under: the project, its policy, which chats are open, and the
/// audit.
pub struct Ground<'a> {
    pub root: &'a Path,
    pub locks: &'a sandbox::policy::Locks,
    /// Whether chat `session` is one this app has open now.
    pub is_open: &'a dyn Fn(u32) -> bool,
    /// Whether this app started chat `session` inside a sandbox: its own record of what the
    /// chat's sandbox was compiled to, never a word the chat sent (#1446).
    pub sandboxed: &'a dyn Fn(u32) -> bool,
    /// Writes the audit of a grant or revoke, and answers whether it was written.
    pub audit: Audit<'a>,
    /// Now, in seconds since 1970.
    pub at: u64,
}

impl Store {
    /// Hands every answer to `answers` from now on: the dispatch core's to register.
    pub fn answers_with(&self, answers: Answers) {
        *lock(&self.answers) = Some(answers);
    }

    /// The grants made for the chat whose id is `id`.
    fn chat_pairs(&self, id: Option<&str>) -> Vec<ChatPair> {
        id.and_then(|id| lock(&self.chat).get(id).cloned())
            .unwrap_or_default()
            .into_iter()
            .map(|made| made.pair)
            .collect()
    }

    /// The grants in force for `asking` in the project at `root`: what the dispatch decision
    /// reads to put a limit before a question to the person. Reading it grants nothing; the
    /// entry points below are still what a dispatch starts by.
    pub fn in_force(&self, root: &Path, asking: &Asking) -> InForce {
        InForce::read(root, self.chat_pairs(asking.id.as_deref()))
    }

    /// **A chat asked to dispatch to `target` with `brief`**: covered, held for the person, or
    /// locked. `asking` is the app's record of the chat. Asked twice for the same target while
    /// the first waits, it is the same held dispatch: one Notice, showing the first brief.
    /// With [`Uncovered::Refuse`] nothing is ever held: what is not covered is refused, in the
    /// one place a chat nobody is at is answered ([`crate::dispatchunattended::unattended`]),
    /// which reads no grant made for one chat. A chat that runs on another chat's grants is
    /// refused whatever it asks.
    pub fn request(
        &self,
        ground: &Ground<'_>,
        asking: Asking,
        target: &str,
        brief: &str,
        uncovered: Uncovered,
    ) -> (Requested, Option<Pending>) {
        if uncovered == Uncovered::Refuse {
            // Nothing is held and nothing is raised: the answer is whole, here and now.
            return (
                crate::dispatchunattended::unattended(
                    ground.root,
                    ground.locks,
                    &asking,
                    crate::dispatchunattended::Runs {
                        holds_anothers: asking.held,
                        sandboxed: (ground.sandboxed)(asking.session),
                    },
                    target,
                ),
                None,
            );
        }
        if asking.held {
            return (
                Requested::Refused(HELD_DISPATCHES_TO_NO_ONE.to_owned()),
                None,
            );
        }
        if !purlis_core::personas::valid_name(target) {
            return (
                Requested::Refused(format!(
                    "{} is not a persona's name, so there is nothing to dispatch to.",
                    purlis_core::shown::short(target)
                )),
                None,
            );
        }
        let grants = self.in_force(ground.root, &asking);
        match dispatchgrant::covers(asking.persona.as_deref(), target, &grants, ground.locks) {
            Covers::Covered => (
                Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat(target)),
                None,
            ),
            Covers::Locked(why) => {
                let told = self.hold(ground, asking, target, brief, Some(why.clone()));
                (Requested::Locked(why), told.ok().map(|(held, _)| held))
            }
            // The person's never: refused in a sentence, nothing held and nobody asked.
            Covers::Never => (
                Requested::Refused(dispatchgrant::never_said(
                    asking.persona.as_deref().unwrap_or_default(),
                    target,
                )),
                None,
            ),
            // Read by the decision, with who is above the chat (`covers_in_chain`); said the
            // same way here, should a caller ever hand one on.
            Covers::NeverAbove(above) => (
                Requested::Refused(dispatchgrant::never_above_said(&above, target)),
                None,
            ),
            // The person said no on this chat's tab already: their answer, and no second
            // question.
            Covers::NeedsGrant | Covers::Unread if self.is_kept_blocked(&asking, target) => {
                (Requested::Refused(kept_blocked_said(target)), None)
            }
            // The record of nevers does not read, so no grant counts; this one dispatch the
            // person allowed all the same, having been told so, and it starts once.
            Covers::Unread if self.spend_once(&asking, target) => (
                Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat(target)),
                None,
            ),
            // Nothing covers it, or nothing counts until the record of nevers reads: the
            // person is asked ([`DispatchPending::never_unread`] says which).
            Covers::NeedsGrant | Covers::Unread => {
                match self.hold(ground, asking, target, brief, None) {
                    Ok((held, new)) => (
                        Requested::NeedsGrant { pending: held.id },
                        new.then_some(held),
                    ),
                    Err(why) => (Requested::Refused(why), None),
                }
            }
        }
    }

    /// Spends the one start the person allowed `asking` to `target` while the record of
    /// nevers did not read. Answers whether there was one.
    fn spend_once(&self, asking: &Asking, target: &str) -> bool {
        let mut once = lock(&self.once);
        let wanted = (
            Whose::of(asking),
            ChatPair {
                asking: asking.persona.clone(),
                target: target.to_owned(),
            },
        );
        match once.iter().position(|one| *one == wanted) {
            Some(at) => {
                once.remove(at);
                true
            }
            None => false,
        }
    }

    /// Holds a dispatch for the person, or answers the one already held for that chat and
    /// target, and whether it is new.
    fn hold(
        &self,
        ground: &Ground<'_>,
        asking: Asking,
        target: &str,
        brief: &str,
        locked: Option<String>,
    ) -> Result<(Pending, bool), String> {
        let mut pending = lock(&self.pending);
        pending.retain(|one| (ground.is_open)(one.asking.session));
        if let Some(held) = pending
            .iter()
            .find(|one| one.asking.session == asking.session && one.target == target)
        {
            return Ok((held.clone(), false));
        }
        let waiting = pending
            .iter()
            .filter(|one| one.asking.session == asking.session)
            .count();
        if waiting >= MOST_WAITING_PER_CHAT {
            return Err(format!(
                "{waiting} dispatches from this chat are waiting on the person already, and \
                 purlis holds no more than that. Wait for an answer to one of them."
            ));
        }
        let held = Pending {
            id: self.next.fetch_add(1, Ordering::Relaxed) + 1,
            asking,
            target: target.to_owned(),
            brief: dispatchgrant::shown_brief(brief),
            locked,
            at: ground.at,
        };
        pending.push(held.clone());
        Ok((held, true))
    }

    /// The dispatches of chat `session` waiting on the person, oldest first.
    pub fn waiting(&self, session: u32) -> Vec<Pending> {
        lock(&self.pending)
            .iter()
            .filter(|one| one.asking.session == session)
            .cloned()
            .collect()
    }

    /// Takes the held dispatch `id` out, if it is still held.
    fn take(&self, id: u32) -> Option<Pending> {
        let mut pending = lock(&self.pending);
        let at = pending.iter().position(|one| one.id == id)?;
        Some(pending.remove(at))
    }

    fn answer(&self, answered: &Answered) {
        let answers = lock(&self.answers).clone();
        if let Some(answers) = answers {
            answers(answered);
        }
    }

    /// **Allow, at `level`, on the Notice of held dispatch `id`**: judged again under policy,
    /// audited, then kept where that level keeps it. The held dispatch, and every other one
    /// the grant now covers, is handed on to start.
    pub fn allow(&self, ground: &Ground<'_>, id: u32, level: Level) -> Result<String, String> {
        let gone = || {
            "That dispatch is no longer waiting, so nothing was allowed. Its chat may have \
             closed."
                .to_owned()
        };
        let held = lock(&self.pending)
            .iter()
            .find(|one| one.id == id)
            .cloned()
            .ok_or_else(gone)?;
        if !(ground.is_open)(held.asking.session) {
            self.take(id);
            return Err(gone());
        }
        let asking = held.asking.persona.as_deref();
        if let Some(why) = ground.locks.dispatch_refused(asking, &held.target) {
            return Err(why);
        }
        // A never said since the question was raised (#1503): no Allow on a Notice lifts it.
        // The question goes, its chat is told nothing started, and nothing is granted.
        if self
            .in_force(ground.root, &held.asking)
            .refuses(asking, &held.target)
        {
            if let Some(pending) = self.take(id) {
                self.answer(&Answered {
                    pending,
                    allowed: None,
                });
            }
            return Err(format!(
                "You said never to {} chats dispatching to {} on this machine, so nothing was \
                 allowed. Lift it in {} first.",
                purlis_core::shown::short(asking.unwrap_or_default()),
                purlis_core::shown::short(&held.target),
                dispatchgrant::SETTINGS
            ));
        }
        // What is kept, checked before anything is audited.
        enum Kept {
            Chat(String),
            Pair(Pair),
        }
        let kept = match (level, asking, held.asking.id.as_deref()) {
            (Level::Chat, _, Some(chat)) => Kept::Chat(chat.to_owned()),
            (Level::Chat, _, None) => {
                return Err(
                    "That chat has no id yet, so purlis can allow nothing for it alone.".to_owned(),
                );
            }
            (Level::You | Level::Project, Some(asking), _) => {
                Kept::Pair(Pair::new(asking, &held.target)?)
            }
            (Level::You | Level::Project, None, _) => {
                return Err(
                    "This chat runs as no persona, so there is no pair to allow beyond this \
                     chat. Allow it for this chat."
                        .to_owned(),
                );
            }
        };
        // Whether it can be written is asked before it is recorded, so the log holds no grant
        // that was refused.
        if let (Level::Project, Kept::Pair(pair)) = (level, &kept) {
            purlis_core::settings::dispatch::can_grant(ground.root, pair)?;
        }
        let audited = dispatchgrant::Audited {
            act: dispatchgrant::Act::Grant,
            asking,
            target: &held.target,
            level,
        };
        (ground.audit)(Some(held.asking.session), &audited)?;
        // A write that fails all the same is recorded as taken back, so the log never ends on
        // a grant that is not there.
        let not_kept = |why: String| {
            if let Err(unsaid) = (ground.audit)(
                Some(held.asking.session),
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::Revoke,
                    ..audited
                },
            ) {
                tracing::warn!(
                    "purlis: a dispatch grant that was not kept is still recorded as made \
                     ({unsaid})"
                );
            }
            why
        };
        match &kept {
            Kept::Chat(chat) => {
                let pair = ChatPair {
                    asking: held.asking.persona.clone(),
                    target: held.target.clone(),
                };
                let mut made = lock(&self.chat);
                let mine = made.entry(chat.clone()).or_default();
                if !mine.iter().any(|one| one.pair == pair) {
                    mine.push(ChatMade {
                        pair,
                        at: ground.at,
                        chat: held.asking.name.clone(),
                    });
                }
            }
            Kept::Pair(pair) => {
                if level == Level::Project {
                    purlis_core::settings::dispatch::grant(ground.root, pair).map_err(not_kept)?;
                } else {
                    sandbox::local::grant_dispatch(ground.root, &pair.asking, &pair.target)
                        .map_err(|why| {
                            not_kept(format!("purlis could not keep the grant: {why}"))
                        })?;
                }
                if let Err(why) = sandbox::local::record_made(
                    ground.root,
                    sandbox::local::Made {
                        what: WHAT.to_owned(),
                        target: pair.to_string(),
                        level: level.word().to_owned(),
                        at: ground.at,
                        chat: Some(held.asking.name.clone()),
                    },
                ) {
                    // The grant stands, and is audited; only the list's "when" is lost.
                    tracing::warn!(
                        "purlis: a dispatch grant was kept without when it was made ({why})"
                    );
                }
            }
        }
        self.start_what_is_covered(ground);
        // The record of nevers does not read, so the grant just kept covers nothing yet. The
        // person read that on the Notice and allowed this dispatch: it starts, once, and the
        // next one asks again until the record reads.
        if self.in_force(ground.root, &held.asking).never_unread {
            if let Some(pending) = self.take(id) {
                lock(&self.once).push((
                    Whose::of(&pending.asking),
                    ChatPair {
                        asking: pending.asking.persona.clone(),
                        target: pending.target.clone(),
                    },
                ));
                self.answer(&Answered {
                    allowed: Some(dispatchgrant::grants_for_a_dispatched_chat(&pending.target)),
                    pending,
                });
            }
            return Ok(format!(
                "Allowed {}. This dispatch starts now. The next one asks again until the list \
                 of pairs you said never to reads.",
                level.said()
            ));
        }
        Ok(format!(
            "Allowed {}. The dispatch starts now, and the next one starts without asking.",
            level.said()
        ))
    }

    /// Hands on every held dispatch a grant now covers.
    fn start_what_is_covered(&self, ground: &Ground<'_>) {
        let waiting: Vec<Pending> = lock(&self.pending).clone();
        for held in waiting {
            let grants = self.in_force(ground.root, &held.asking);
            let covered = dispatchgrant::covers(
                held.asking.persona.as_deref(),
                &held.target,
                &grants,
                ground.locks,
            ) == Covers::Covered;
            if covered && let Some(pending) = self.take(held.id) {
                self.answer(&Answered {
                    allowed: Some(dispatchgrant::grants_for_a_dispatched_chat(&pending.target)),
                    pending,
                });
            }
        }
    }

    /// **Chat `session` closed.** What it had waiting on the person goes with it, and where no
    /// session of the chat is still open, `id` names it and its "this chat" grants end
    /// (D-1348-1's rule for its sandbox grants: a restart starts the new run before the old
    /// one ends, so a restarted chat keeps them, D-1437-R3). Nothing is told to anyone: there
    /// is no chat left to tell.
    pub fn chat_closed(&self, session: u32, id: Option<&str>) {
        lock(&self.pending).retain(|one| one.asking.session != session);
        let mut kept = lock(&self.kept_blocked);
        let mut once = lock(&self.once);
        kept.remove(&Whose::Session(session));
        once.retain(|(whose, _)| *whose != Whose::Session(session));
        if let Some(id) = id {
            lock(&self.chat).remove(id);
            kept.remove(&Whose::Id(id.to_owned()));
            once.retain(|(whose, _)| *whose != Whose::Id(id.to_owned()));
        }
    }

    /// Whether the person kept `asking`'s dispatch to `target` blocked on its tab.
    fn is_kept_blocked(&self, asking: &Asking, target: &str) -> bool {
        lock(&self.kept_blocked)
            .get(&Whose::of(asking))
            .is_some_and(|pairs| {
                pairs
                    .iter()
                    .any(|pair| pair.asking == asking.persona && pair.target == target)
            })
    }

    /// **The person allowed `shown` on the project's Notice** (D-1437-R1): each pair of it the
    /// committed file holds and this machine had not acknowledged is audited as a grant for
    /// everyone, then in force here, and every held dispatch it covers is handed on. A pair
    /// the file does not hold is nothing. What the file no longer holds is taken off what was
    /// acknowledged, so its going is told once.
    pub fn acknowledge(&self, ground: &Ground<'_>, shown: &[String]) -> Result<(), String> {
        for pair in dispatchgrant::unacknowledged(ground.root) {
            if !shown.contains(&pair.to_string()) {
                continue;
            }
            (ground.audit)(
                None,
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::Grant,
                    asking: Some(&pair.asking),
                    target: &pair.target,
                    level: Level::Project,
                },
            )?;
            dispatchgrant::acknowledge_pair(ground.root, &pair)
                .map_err(|why| format!("purlis could not record it as allowed: {why}"))?;
        }
        let committed: Vec<String> = dispatchgrant::committed_at(ground.root)
            .iter()
            .map(ToString::to_string)
            .collect();
        if let Some(seen) = sandbox::local::dispatch_seen(ground.root) {
            let kept: Vec<String> = seen
                .iter()
                .filter(|one| committed.contains(one))
                .cloned()
                .collect();
            if kept != seen {
                dispatchgrant::acknowledge(ground.root, &kept)
                    .map_err(|why| format!("purlis could not record what was read: {why}"))?;
            }
        }
        self.start_what_is_covered(ground);
        Ok(())
    }

    /// **Keep blocked** on the Notice of held dispatch `id`: it goes, nothing is granted, and
    /// **the answer holds for that chat's life** (#1503): the same chat asking across the same
    /// pair again is refused at once ([`kept_blocked_said`]) and the person is not asked
    /// again. Another chat, and this one once it is closed and opened anew, is asked. Answers
    /// whether it was held.
    ///
    /// Putting away the Notice of a dispatch policy locked is no answer to a question, and
    /// nothing is remembered for it.
    pub fn keep_blocked(&self, id: u32) -> bool {
        let Some(pending) = self.take(id) else {
            return false;
        };
        if pending.locked.is_none() {
            let pair = ChatPair {
                asking: pending.asking.persona.clone(),
                target: pending.target.clone(),
            };
            {
                let mut kept = lock(&self.kept_blocked);
                let mine = kept.entry(Whose::of(&pending.asking)).or_default();
                if !mine.contains(&pair) {
                    mine.push(pair);
                }
            }
            self.answer(&Answered {
                pending,
                allowed: None,
            });
        }
        true
    }

    /// **Takes held dispatch `id` back out, with no answer from the person**: the app's own,
    /// for a question it raised and withdraws. Nothing is remembered and nobody is told.
    /// Answers whether it was held.
    pub fn withdraw(&self, id: u32) -> bool {
        self.take(id).is_some()
    }

    /// **Never for this pair**, on the Notice of held dispatch `id` (#1503): audited, then
    /// kept for the person on this machine. From then on no chat running as that persona is
    /// asked or allowed to dispatch to that target, whatever is granted and wherever, until
    /// the person lifts it in Settings. Every dispatch held across the pair, this one and
    /// other chats', goes unstarted and its chat is told.
    pub fn never(&self, ground: &Ground<'_>, id: u32) -> Result<String, String> {
        let gone = || {
            "That dispatch is no longer waiting, so nothing was changed. Its chat may have \
             closed."
                .to_owned()
        };
        let held = lock(&self.pending)
            .iter()
            .find(|one| one.id == id)
            .cloned()
            .ok_or_else(gone)?;
        if !(ground.is_open)(held.asking.session) {
            self.take(id);
            return Err(gone());
        }
        if let Some(why) = &held.locked {
            return Err(why.clone());
        }
        let Some(asking) = held.asking.persona.as_deref() else {
            return Err(
                "This chat runs as no persona, so there is no pair to say never to. Keep it \
                 blocked for this chat."
                    .to_owned(),
            );
        };
        let pair = Pair::new(asking, &held.target)?;
        // A record that does not read is not written over: said before anything is audited.
        if let Some(unread) = dispatchgrant::nevers_unread(ground.root) {
            return Err(unread);
        }
        let audited = dispatchgrant::Audited {
            act: dispatchgrant::Act::Never,
            asking: Some(asking),
            target: &held.target,
            level: Level::You,
        };
        (ground.audit)(Some(held.asking.session), &audited)?;
        if let Err(why) = dispatchgrant::never(ground.root, &pair) {
            // Recorded as lifted, so the log never ends on a never that is not there.
            if let Err(unsaid) = (ground.audit)(
                Some(held.asking.session),
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::LiftNever,
                    ..audited
                },
            ) {
                tracing::warn!(
                    "purlis: a never that was not kept is still recorded as made ({unsaid})"
                );
            }
            return Err(format!("purlis could not keep it: {why}"));
        }
        let across: Vec<u32> = lock(&self.pending)
            .iter()
            .filter(|one| {
                one.locked.is_none()
                    && one.asking.persona.as_deref() == Some(asking)
                    && one.target == held.target
            })
            .map(|one| one.id)
            .collect();
        for id in across {
            if let Some(pending) = self.take(id) {
                self.answer(&Answered {
                    pending,
                    allowed: None,
                });
            }
        }
        Ok(format!(
            "No {asking} chat dispatches to {} on this machine from now on, and you are not \
             asked again. Lift it in {}.",
            held.target,
            dispatchgrant::SETTINGS
        ))
    }

    /// Every grant made for a chat that is open, oldest first, by chat id.
    fn chat_grants(&self, open: &dyn Fn(&str) -> bool) -> Vec<(String, ChatMade)> {
        let mut out: Vec<(String, ChatMade)> = lock(&self.chat)
            .iter()
            .filter(|(id, _)| open(id))
            .flat_map(|(id, made)| made.iter().map(|one| (id.clone(), one.clone())))
            .collect();
        out.sort_by_key(|(_, one)| one.at);
        out
    }

    /// Takes back chat `id`'s grant of `pair`. Answers whether there was one.
    fn revoke_chat(&self, id: &str, pair: &ChatPair) -> bool {
        let mut made = lock(&self.chat);
        let Some(mine) = made.get_mut(id) else {
            return false;
        };
        let before = mine.len();
        mine.retain(|one| one.pair != *pair);
        mine.len() != before
    }
}

/// What a dispatch grant is recorded as in this machine's record of grants made
/// (`sandbox::local::Made`).
const WHAT: &str = "dispatch";

/// The separator of a grant's id: no persona's name or chat id holds it.
const SEP: char = '\u{1f}';

// ---- what the window is told and sends -------------------------------------------------------

/// A dispatch that needs the person, as the Notice on the asking chat's tab shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchPending {
    pub plane: PlaneId,
    /// What Allow and Keep blocked are sent by.
    pub id: u32,
    /// The asking chat.
    pub session: u32,
    /// The asking chat's name, as its tab shows it.
    pub chat: String,
    /// The persona the asking chat runs as; null for a chat on no persona.
    pub asking: Option<String>,
    /// The persona it wants to dispatch to.
    pub target: String,
    /// The first brief, **as the chat wrote it**: never purlis's words, and drawn apart from
    /// them, as plain text.
    pub brief: String,
    /// Whether the brief was longer than the Notice shows and is cut.
    pub brief_cut: bool,
    /// How many lines the brief is, blank ones counted: the Notice says it, since its box
    /// shows only the first of a long one.
    pub brief_lines: u32,
    /// The levels Allow is offered at. Empty where policy locks it.
    pub levels: Vec<GrantLevel>,
    /// Where policy locks it: the policy's sentence, naming who set it. No Allow is offered.
    pub locked: Option<String>,
    /// Where this machine's list of pairs the person said never to does not read (#1503): the
    /// sentence saying so. No grant counts until it reads, which is why the person is asked;
    /// an Allow starts this one dispatch, and the next asks again.
    pub never_unread: Option<String>,
}

/// `held` as the window is told it, in project `plane`.
fn told(plane: &PlaneId, root: &Path, held: &Pending) -> DispatchPending {
    let mut levels = Vec::new();
    if held.locked.is_none() {
        if held.asking.id.is_some() {
            levels.push(GrantLevel::Chat);
        }
        if held.asking.persona.is_some() {
            levels.push(GrantLevel::You);
            if purlis_core::names::has_manifest(root) {
                levels.push(GrantLevel::Project);
            }
        }
    }
    DispatchPending {
        plane: plane.clone(),
        id: held.id,
        session: held.asking.session,
        chat: held.asking.name.clone(),
        asking: held.asking.persona.clone(),
        target: held.target.clone(),
        brief: held.brief.text.clone(),
        brief_cut: held.brief.cut,
        brief_lines: held.brief.lines,
        levels,
        never_unread: held
            .locked
            .is_none()
            .then(|| dispatchgrant::nevers_unread(root))
            .flatten(),
        locked: held.locked.clone(),
    }
}

/// Seconds since 1970, now.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// **THE DISPATCH CORE'S ENTRY POINT, for a chat a person is at (#1436 calls this).**
///
/// Chat `session` of `held`'s project asks to dispatch to persona `target` with `brief`. The
/// asking persona is the one this app's record of chat `session` runs with; nothing in the
/// request names it. Answers whether the dispatch is covered, held for the person (the Notice
/// is raised on the asking chat's tab), locked by policy, or refused. The module's own doc is
/// the contract.
pub fn request_dispatch_grant(
    held: &crate::planes::Held,
    session: u32,
    target: &str,
    brief: &str,
) -> Requested {
    requested(held, session, target, brief, Uncovered::AskThePerson)
}

/// **THE DISPATCH CORE'S ENTRY POINT, for a chat nobody is at (an unattended chat, #1446).**
///
/// [`request_dispatch_grant`], but a dispatch nothing covers is a plain refusal: nothing is
/// held, no Notice is raised, and a locked one raises none either. So an unattended chat
/// dispatches only under a grant that already exists and that this machine has acknowledged,
/// and a missing grant is a sentence for the chat, never a prompt nobody is there to answer.
pub fn request_dispatch_grant_or_refuse(
    held: &crate::planes::Held,
    session: u32,
    target: &str,
    brief: &str,
) -> Requested {
    requested(held, session, target, brief, Uncovered::Refuse)
}

/// Both entry points.
fn requested(
    held: &crate::planes::Held,
    session: u32,
    target: &str,
    brief: &str,
    uncovered: Uncovered,
) -> Requested {
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
    let locks = sandbox::policy::Locks::of(root);
    let (answer, raised) = held.dispatch_grants().request(
        &Ground {
            root,
            locks: &locks,
            is_open: &|session| held.chats().recorded_chat(session).is_some(),
            sandboxed: &|session| held.chats().confines_of(session).is_some(),
            audit: &|number, audited| held.hooks().record_dispatch_grant(root, number, audited),
            at: now_secs(),
        },
        asking,
        target,
        brief,
        uncovered,
    );
    if let (Some(raised), Some(tell)) = (raised, TELL.get()) {
        tell(told(held.plane_id(), root, &raised));
    }
    answer
}

/// The dispatches of chat `session` waiting on the person, for the Notice on its tab.
#[tauri::command]
#[specta::specta]
pub fn dispatch_grants_needed(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Vec<DispatchPending>, String> {
    let held = planes.held(&plane)?;
    Ok(held
        .dispatch_grants()
        .waiting(session)
        .iter()
        .map(|one| told(&plane, held.root(), one))
        .collect())
}

/// What allowing a dispatch answered: the sentence the Notice says.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchAllowed {
    pub said: String,
}

/// **Allow** on a dispatch's Notice, at `level`: the pair is the app's record of the held
/// dispatch `id`, never the window's word. Audited, then kept where that level keeps it, and
/// the dispatch starts.
#[tauri::command]
#[specta::specta]
pub fn allow_dispatch(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: u32,
    level: GrantLevel,
) -> Result<DispatchAllowed, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    let locks = sandbox::policy::Locks::of(root);
    held.dispatch_grants()
        .allow(
            &Ground {
                root,
                locks: &locks,
                is_open: &|session| held.chats().recorded_chat(session).is_some(),
                sandboxed: &|session| held.chats().confines_of(session).is_some(),
                audit: &|number, audited| held.hooks().record_dispatch_grant(root, number, audited),
                at: now_secs(),
            },
            id,
            level.into(),
        )
        .map(|said| DispatchAllowed { said })
}

/// **Keep blocked** on a dispatch's Notice: nothing is granted, and the dispatch does not
/// start. Answers whether it was still waiting.
#[tauri::command]
#[specta::specta]
pub fn keep_dispatch_blocked(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: u32,
) -> Result<bool, String> {
    Ok(planes.held(&plane)?.dispatch_grants().keep_blocked(id))
}

/// **Never for this pair** on a dispatch's Notice (#1503): the pair is the app's record of the
/// held dispatch `id`, never the window's word. Audited, then kept for the person on this
/// machine; the dispatch does not start, and no chat of that persona is asked for that target
/// again until it is lifted ([`lift_dispatch_never`]).
#[tauri::command]
#[specta::specta]
pub fn never_dispatch(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: u32,
) -> Result<DispatchAllowed, String> {
    let held = planes.held(&plane)?;
    with_ground(&held, |ground| held.dispatch_grants().never(ground, id))
        .map(|said| DispatchAllowed { said })
}

/// The ground a window command of `held`'s project stands on.
fn with_ground<T>(held: &crate::planes::Held, with: impl FnOnce(&Ground<'_>) -> T) -> T {
    let root = held.root();
    let locks = sandbox::policy::Locks::of(root);
    with(&Ground {
        root,
        locks: &locks,
        is_open: &|session| held.chats().recorded_chat(session).is_some(),
        sandboxed: &|session| held.chats().confines_of(session).is_some(),
        audit: &|number, audited| held.hooks().record_dispatch_grant(root, number, audited),
        at: now_secs(),
    })
}

/// A pair the person said never to, as Settings lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchNever {
    pub asking: String,
    pub target: String,
}

/// One persona whose chats may dispatch to any persona, as Settings lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchAny {
    pub asking: String,
    /// `you` or `project`: where it is kept.
    pub level: GrantLevel,
    /// Whether it is the project's and nobody on this machine has accepted it yet: it covers
    /// nothing here until it is allowed here, in Settings.
    pub waiting: bool,
}

/// What stands beside the named grants (#1503): the pairs the person said never to on this
/// machine, and the personas whose chats may dispatch to any persona.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchStanding {
    pub nevers: Vec<DispatchNever>,
    pub any: Vec<DispatchAny>,
    /// Where the list of nevers is there and does not read: the sentence saying so, and how
    /// the person mends it. `nevers` is then empty, and no dispatch grant counts.
    pub nevers_unread: Option<String>,
}

/// What stands in the project at `root`.
fn standing_of(root: &Path) -> DispatchStanding {
    let unaccepted = dispatchgrant::any_unaccepted(root);
    let mut any: Vec<DispatchAny> = dispatchgrant::any_yours(root)
        .into_iter()
        .map(|asking| DispatchAny {
            asking,
            level: GrantLevel::You,
            waiting: false,
        })
        .collect();
    any.extend(
        dispatchgrant::any_committed_at(root)
            .into_iter()
            .map(|asking| DispatchAny {
                waiting: unaccepted.contains(&asking),
                asking,
                level: GrantLevel::Project,
            }),
    );
    DispatchStanding {
        nevers: dispatchgrant::nevers(root)
            .into_iter()
            .map(|(asking, target)| DispatchNever { asking, target })
            .collect(),
        any,
        nevers_unread: dispatchgrant::nevers_unread(root),
    }
}

/// **Lifts the never for `asking` to `target`** in the project at `root`: checked to be
/// there, audited, then taken out. What then covers the pair is whatever grant stands; with
/// none, the next dispatch asks.
fn lift_never(root: &Path, asking: &str, target: &str, audit: Audit<'_>) -> Result<(), String> {
    // A record that does not read is not written over: said before anything is audited.
    if let Some(unread) = dispatchgrant::nevers_unread(root) {
        return Err(unread);
    }
    let there = dispatchgrant::nevers(root)
        .iter()
        .any(|(from, to)| from == asking && to == target);
    if !there {
        return Err("purlis lifted nothing: that never is no longer there.".to_owned());
    }
    audit(
        None,
        &dispatchgrant::Audited {
            act: dispatchgrant::Act::LiftNever,
            asking: Some(asking),
            target,
            level: Level::You,
        },
    )?;
    dispatchgrant::lift_never(root, asking, target)
        .map(|_| ())
        .map_err(|why| format!("purlis could not lift it: {why}"))
}

/// **Lets chats running as `asking` dispatch to any persona**, at `level`, in the project at
/// `root` where `known` says which names are its personas: checked, audited with the target
/// `*`, then kept. Settings' explicit grant, which no Notice's answer reaches.
fn allow_any(
    root: &Path,
    known: &dyn Fn(&str) -> bool,
    asking: &str,
    level: Level,
    audit: Audit<'_>,
) -> Result<(), String> {
    if !known(asking) {
        return Err(format!(
            "This project has no persona named {}, so nothing was granted.",
            purlis_core::shown::short(asking)
        ));
    }
    dispatchgrant::can_allow_any(root, asking, level)?;
    let audited = dispatchgrant::Audited {
        act: dispatchgrant::Act::Grant,
        asking: Some(asking),
        target: dispatchgrant::ANY,
        level,
    };
    audit(None, &audited)?;
    dispatchgrant::allow_any(root, asking, level).inspect_err(|_| {
        // Recorded as taken back, so the log never ends on a grant that is not there.
        if let Err(unsaid) = audit(
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
    })
}

/// **Takes back `asking`'s grant of any persona at `level`**: checked to be there, audited,
/// then taken out. A chat already running is left as it is.
fn revoke_any(root: &Path, asking: &str, level: Level, audit: Audit<'_>) -> Result<(), String> {
    let there = match level {
        Level::Chat => false,
        Level::You => dispatchgrant::any_yours(root)
            .iter()
            .any(|one| one == asking),
        Level::Project => dispatchgrant::any_committed_at(root)
            .iter()
            .any(|one| one == asking),
    };
    if !there {
        return Err("purlis did not revoke it: that grant is no longer there.".to_owned());
    }
    audit(
        None,
        &dispatchgrant::Audited {
            act: dispatchgrant::Act::Revoke,
            asking: Some(asking),
            target: dispatchgrant::ANY,
            level,
        },
    )?;
    dispatchgrant::revoke_any(root, asking, level).map(|_| ())
}

/// What stands beside the named dispatch grants here: the pairs you said never to, and the
/// personas whose chats may dispatch to any persona. For Settings' list.
#[tauri::command]
#[specta::specta]
pub fn dispatch_standing(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<DispatchStanding, String> {
    Ok(standing_of(planes.held(&plane)?.root()))
}

/// **Lift** on Settings' list of the pairs you said never to: audited, then taken out, so the
/// next dispatch across the pair is covered by whatever grant stands, or asks. Answers what
/// stands now.
#[tauri::command]
#[specta::specta]
pub fn lift_dispatch_never(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    lift_never(root, &asking, &target, &|number, audited| {
        held.hooks().record_dispatch_grant(root, number, audited)
    })?;
    Ok(standing_of(root))
}

/// **Any persona**, from Settings: chats running as `asking` may dispatch to every persona of
/// the project, one added later included, for you on this machine or for everyone in the
/// project. Audited, then kept; a dispatch waiting on the person that it covers starts.
/// Answers what stands now.
#[tauri::command]
#[specta::specta]
pub fn allow_dispatch_to_any(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    level: GrantLevel,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    let personas = purlis_core::workspaces::Plane::open(root.to_path_buf())
        .personas()
        .unwrap_or_default();
    with_ground(&held, |ground| {
        allow_any(
            root,
            &|name| personas.iter().any(|one| one == name),
            &asking,
            level.into(),
            ground.audit,
        )?;
        held.dispatch_grants().start_what_is_covered(ground);
        Ok::<(), String>(())
    })?;
    Ok(standing_of(root))
}

/// **Revoke** on Settings' list of any-persona grants: audited, then taken out. Answers what
/// stands now.
#[tauri::command]
#[specta::specta]
pub fn revoke_dispatch_to_any(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    level: GrantLevel,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    revoke_any(root, &asking, level.into(), &|number, audited| {
        held.hooks().record_dispatch_grant(root, number, audited)
    })?;
    Ok(standing_of(root))
}

/// One dispatch grant, as Settings lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchGrant {
    /// What Revoke is sent by.
    pub id: String,
    /// The asking persona; null for a grant made for a chat on no persona.
    pub asking: Option<String>,
    pub target: String,
    pub level: GrantLevel,
    /// Who committed it, for one the project carries; null for one you granted, or one the
    /// project's file holds that is not committed yet.
    pub by: Option<String>,
    /// When, in seconds since 1970, where that is known.
    pub at: Option<u32>,
    /// The chat it was allowed from, where that is known.
    pub chat: Option<String>,
    /// Why a policy locks it out, where one does: it covers nothing while it is locked.
    pub locked: Option<String>,
    /// Whether it is the project's and nobody on this machine has allowed it yet: it covers
    /// nothing here until the project's Notice, or a chat's, is answered (D-1437-R1).
    pub waiting: bool,
}

/// A pair policy locks, as Settings shows it locked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchLock {
    pub asking: String,
    pub target: String,
}

/// The project's dispatch grants as they changed since this machine last told the person.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchGrantsChanged {
    /// Each as `asking -> target`.
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// The whole list now: what the Notice sends back once it is read.
    pub now: Vec<String>,
}

/// Everything Settings shows of dispatch grants: the grants, what policy locks, and the
/// teammate's one-time change.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchGrants {
    pub grants: Vec<DispatchGrant>,
    /// Where policy forbids every dispatch: its sentence, naming who set it.
    pub all_locked: Option<String>,
    /// The pairs policy locks.
    pub locked_pairs: Vec<DispatchLock>,
    /// "Locked by policy, set by <who> in <file>.", where a policy locks anything of dispatch.
    pub locked_by: Option<String>,
    /// How the project's grants changed since this machine last told the person; null when
    /// nothing did.
    pub changed: Option<DispatchGrantsChanged>,
}

/// Who last committed the line granting `pair` in the project's committed file, and when: the
/// project's history, asked of git. `None` where git has nothing to say (not committed yet).
fn committed_by(root: &Path, pair: &Pair) -> Option<(String, u64)> {
    let manifest = purlis_core::names::manifest(root);
    let file = manifest.file_name()?.to_str()?.to_owned();
    // A persona's name is letters, digits, dots, underscores and hyphens: only the dot means
    // anything to the pattern.
    let plain = |name: &str| name.replace('.', "\\.");
    let mut git = std::process::Command::new("git");
    git.arg("-C")
        .arg(root)
        .args(["log", "-1", "--format=%an%x09%at", "-G"])
        .arg(format!(
            "^[[:space:]]*\"?{}\"?[[:space:]]*=.*\"{}\"",
            plain(&pair.asking),
            plain(&pair.target)
        ))
        .args(["--", &file]);
    let out = purlis_core::forklock::output(&mut git).ok()?;
    let line = String::from_utf8(out.stdout).ok()?;
    let (name, at) = line.trim().split_once('\t')?;
    Some((name.to_owned(), at.parse().ok()?))
}

/// **Every dispatch grant in force for the project at `root`**: each open chat's (a closed
/// chat's ended with it), yours on this machine, and the project's.
fn grants_of(
    root: &Path,
    store: &Store,
    locks: &sandbox::policy::Locks,
    open: &dyn Fn(&str) -> bool,
) -> Vec<DispatchGrant> {
    let made = sandbox::local::made(root);
    let recorded = |pair: &Pair, level: Level| {
        made.iter()
            .find(|one| {
                one.what == WHAT && one.target == pair.to_string() && one.level == level.word()
            })
            .cloned()
    };
    let locked = |asking: Option<&str>, target: &str| locks.dispatch_refused(asking, target);
    let unseen = dispatchgrant::unacknowledged(root);
    let mut out: Vec<DispatchGrant> = store
        .chat_grants(open)
        .into_iter()
        .map(|(id, one)| {
            let asking = one.pair.asking.clone();
            DispatchGrant {
                id: format!(
                    "chat{SEP}{id}{SEP}{}{SEP}{}",
                    asking.as_deref().unwrap_or_default(),
                    one.pair.target
                ),
                locked: locked(asking.as_deref(), &one.pair.target),
                asking,
                target: one.pair.target,
                level: GrantLevel::Chat,
                by: None,
                at: u32::try_from(one.at).ok(),
                chat: Some(one.chat),
                waiting: false,
            }
        })
        .collect();
    let row = |pair: &Pair, level: Level| {
        let record = recorded(pair, level);
        DispatchGrant {
            id: format!("{}{SEP}{}{SEP}{}", level.word(), pair.asking, pair.target),
            asking: Some(pair.asking.clone()),
            target: pair.target.clone(),
            level: level.into(),
            by: None,
            at: record.as_ref().and_then(|one| u32::try_from(one.at).ok()),
            chat: record.and_then(|one| one.chat),
            locked: locked(Some(&pair.asking), &pair.target),
            waiting: false,
        }
    };
    out.extend(
        dispatchgrant::yours(root)
            .iter()
            .map(|pair| row(pair, Level::You)),
    );
    for pair in dispatchgrant::committed_at(root) {
        let committed = committed_by(root, &pair);
        let mut one = row(&pair, Level::Project);
        one.waiting = unseen.contains(&pair);
        if let Some((name, at)) = committed {
            one.at = u32::try_from(at).ok().or(one.at);
            one.by = Some(name);
        }
        out.push(one);
    }
    out
}

/// **Revokes the grant called `id`** ([`grants_of`]) in the project at `root`: checked to be
/// there, audited, then taken out. The next dispatch across the pair asks again; a persona
/// chat already running is left as it is. A project grant's revoke is a change to the
/// committed file, which teammates follow.
fn revoke(root: &Path, store: &Store, id: &str, audit: Audit<'_>) -> Result<(), String> {
    let gone = || "purlis did not revoke it: that grant is no longer there.".to_owned();
    let parts: Vec<&str> = id.split(SEP).collect();
    match parts.as_slice() {
        ["chat", chat, asking, target] => {
            let pair = ChatPair {
                asking: (!asking.is_empty()).then(|| (*asking).to_owned()),
                target: (*target).to_owned(),
            };
            if !store.chat_pairs(Some(chat)).contains(&pair) {
                return Err(gone());
            }
            audit(
                None,
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::Revoke,
                    asking: pair.asking.as_deref(),
                    target,
                    level: Level::Chat,
                },
            )?;
            store.revoke_chat(chat, &pair);
            Ok(())
        }
        [level, asking, target] => {
            let level = Level::of_word(level)
                .filter(|level| *level != Level::Chat)
                .ok_or_else(gone)?;
            let pair = Pair::new(asking, target).map_err(|_| gone())?;
            let there = if level == Level::Project {
                dispatchgrant::committed_at(root).contains(&pair)
            } else {
                dispatchgrant::yours(root).contains(&pair)
            };
            if !there {
                return Err(gone());
            }
            audit(
                None,
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::Revoke,
                    asking: Some(&pair.asking),
                    target: &pair.target,
                    level,
                },
            )?;
            if level == Level::Project {
                purlis_core::settings::dispatch::revoke(root, &pair)?;
            } else {
                sandbox::local::revoke_dispatch(root, &pair.asking, &pair.target)
                    .map_err(|why| format!("purlis could not revoke it: {why}"))?;
            }
            if let Err(why) =
                sandbox::local::forget_made(root, WHAT, &pair.to_string(), level.word())
            {
                tracing::warn!("purlis: a revoked dispatch grant's record was left behind ({why})");
            }
            Ok(())
        }
        _ => Err(gone()),
    }
}

/// Everything Settings shows of dispatch grants in the project at `root`.
fn state_of(root: &Path, store: &Store, open: &dyn Fn(&str) -> bool) -> DispatchGrants {
    let locks = sandbox::policy::Locks::of(root);
    let locks_dispatch = locks.forbids_dispatch() || !locks.locked_pairs().is_empty();
    DispatchGrants {
        grants: grants_of(root, store, &locks, open),
        all_locked: locks
            .forbids_dispatch()
            .then(|| locks.dispatch_refused(None, "").unwrap_or_default()),
        locked_pairs: locks
            .locked_pairs()
            .iter()
            .map(|(asking, target)| DispatchLock {
                asking: asking.clone(),
                target: target.clone(),
            })
            .collect(),
        locked_by: locks_dispatch.then(|| locks.locked_by()),
        changed: changed_of(root),
    }
}

fn changed_of(root: &Path) -> Option<DispatchGrantsChanged> {
    dispatchgrant::changed(root).map(|change| DispatchGrantsChanged {
        added: change.added,
        removed: change.removed,
        now: change.now,
    })
}

/// The ids of the chats `held` has open.
fn open_ids(held: &crate::planes::Held) -> std::collections::HashSet<String> {
    held.chats()
        .open_now()
        .into_iter()
        .filter_map(|open| held.chats().recorded_chat(open.session)?.identity.id)
        .collect()
}

/// Every dispatch grant in force here, what policy locks, and the teammate's one-time change:
/// for Settings' list and the project's Notice.
#[tauri::command]
#[specta::specta]
pub fn dispatch_grants(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<DispatchGrants, String> {
    let held = planes.held(&plane)?;
    let open = open_ids(&held);
    Ok(state_of(held.root(), held.dispatch_grants(), &|id| {
        open.contains(id)
    }))
}

/// **Revoke** on Settings' list of dispatch grants: the grant called `id` is audited and taken
/// out, so the next dispatch across its pair asks again. Answers the list as it is now.
#[tauri::command]
#[specta::specta]
pub fn revoke_dispatch_grant(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: String,
) -> Result<DispatchGrants, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    revoke(root, held.dispatch_grants(), &id, &|number, audited| {
        held.hooks().record_dispatch_grant(root, number, audited)
    })?;
    let open = open_ids(&held);
    Ok(state_of(root, held.dispatch_grants(), &|id| {
        open.contains(id)
    }))
}

/// **Allow** on the Notice of the project's dispatch grants (D-1437-R1): `shown` is the pairs
/// the person allowed, as the Notice showed them, all of them or one. Each the committed file
/// holds is audited and is in force on this machine from now on; what the file no longer
/// holds is read. Answers what is still to tell, if anything.
#[tauri::command]
#[specta::specta]
pub fn acknowledge_dispatch_grants(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    shown: Vec<String>,
) -> Result<Option<DispatchGrantsChanged>, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    let locks = sandbox::policy::Locks::of(root);
    held.dispatch_grants().acknowledge(
        &Ground {
            root,
            locks: &locks,
            is_open: &|session| held.chats().recorded_chat(session).is_some(),
            sandboxed: &|session| held.chats().confines_of(session).is_some(),
            audit: &|number, audited| held.hooks().record_dispatch_grant(root, number, audited),
            at: now_secs(),
        },
        &shown,
    )?;
    Ok(changed_of(root))
}

#[cfg(test)]
#[path = "dispatchgrants_tests.rs"]
mod tests;
