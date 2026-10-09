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
//! **The app's dispatch path calls [`requested_as`]** (#1543), through
//! `dispatchunattended::request_dispatch_as`, with the asking chat as it read it once: the two
//! entry points above are that, for a chat read from `session`, and are what the tests drive.
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
//! - On Allow, and when the person accepts a teammate's committed grant on the Notice that
//!   says it arrived, every held dispatch the grant covers is handed to the listener, which
//!   starts it.
//!
//! # What a chat cannot do
//!
//! **Nothing a chat sends creates, widens or revokes a grant.** A grant is made by
//! [`allow_dispatch`] or [`answer_dispatch_arrival`], window commands a person's press
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
//! **No answer to a Notice makes or accepts one** (V100-23): [`Store::allow`] keeps the one
//! pair the held dispatch names, whose target is a persona's name, and the Notice that says a
//! teammate's grant arrived tells of a project's "any persona" and declines it, and never
//! accepts it ([`answer_dispatch_arrival`] refuses that in the command itself).
//!
//! # A teammate's grant, when it arrives (#1506)
//!
//! A commit by anyone who can push to the project can add a grant to its file. It is in force
//! on this machine for nobody until the person here accepts it. [`dispatch_arrival`] is what
//! waits: the window says it once, at its own level and not on a chat's tab, when the project
//! opens and each time the watcher says the project moved. **Accept** (named pairs only) and
//! **Not on my machine** answer what was listed ([`answer_dispatch_arrival`]): **only what is
//! still exactly as it was shown** is answered, and where the list moved meanwhile the person
//! is told and shown the list as it is. Putting the Notice away answers nothing. A grant that
//! names a persona this checkout does not define is said to, and is never accepted.
//! **Accepting is this machine's act everywhere**: it records the acceptance and never goes
//! through the writer of the committed file.
//!
//! **This machine's yes is bound to what it accepted** ([`purlis_core::dispatcharrival`]): a
//! grant a commit takes out of the project's file is no longer accepted here, and one taken
//! out and put back, even with nothing read in between, waits for a new yes and says why.
//! What a commit took away is told once and asks nothing ([`dispatch_gone_told`]). A grant
//! the file on disk does not hold is not in force, and nothing is dropped or told for that
//! alone, so a branch switched away and back moves nothing.
//!
//! **Where git runs, and under which lock a decision reads.** Settling the acceptances reads
//! git's history, one settling per project at a time. It runs on a blocking thread for the
//! window's reads and answers, on a thread of its own after the watcher's event
//! ([`project_moved`]), and in the dispatch path **before** the lock a dispatch is decided
//! under is taken (`handoff`). Under that lock, and everywhere else the grants in force are
//! read ([`Store::in_force`]), nothing runs git: the read goes by the last settling's
//! verdict, held under `dispatcharrival`'s own lock for the length of a copy. No table read
//! and no watcher event settles on the thread that asked.
//!
//! # The table in Settings (#1504)
//!
//! Settings › Project › Dispatch is where every grant is seen and taken back, and the one
//! place "any persona" is set and cleared. Its commands are the window's alone, each on a
//! person's press: [`revoke_dispatch_grant`], [`lift_dispatch_never`],
//! [`allow_dispatch_to_any`], [`revoke_dispatch_to_any`], [`accept_project_dispatch`],
//! [`decline_project_dispatch`], [`give_back_dispatch`], [`remove_dormant_dispatch`].
//! **Each reads the record it changes again before it writes**: what the window sends is a
//! name, never a view of the table, and a row that is no longer there is refused with a
//! sentence and nothing audited. **Revoking stops new dispatches only**: none of them holds a
//! chat, so a task already running is left as it is.
//!
//! **A grant is in force only while both personas exist** ([`purlis_core::dispatchdormant`]):
//! [`requested`] refuses a dispatch to a name that is no persona, and one from a chat whose
//! persona is no persona of the project now, before any grant is read. Nothing is moved for
//! a persona that is away. A name seen gone that is another persona's now has its grants set
//! aside as the next dispatch is judged ([`bring_up_to_date`]), recorded in the event log as
//! taken back by purlis, and gets them back only by the person's Give back, once for the
//! name. **Reading the table moves nothing** ([`standing_read`]).
//!
//! # What a persona wants, and several pairs in one answer (#1502)
//!
//! A persona's definition may say which personas it usually works with
//! ([`purlis_core::dispatchwants`]). **That line grants nothing**: [`Store::request`] does not
//! read it, so a persona that wants another is asked exactly as one that does not. It is read
//! for the question alone: under its answers the Notice offers a box for each wanted persona
//! that nothing answers for yet, unticked, and says what each persona works with.
//!
//! **An Allow keeps the asked pair and every ticked one at the level chosen**
//! ([`Store::allow_with`]), each audited as its own grant. Keep blocked and Never are about
//! the asked pair only.
//!
//! **A chat can write persona definitions** (its own persona's, and at the project's root any
//! persona's), so what the question offers is something a chat can move. What holds:
//!
//! - the boxes and the sentences are built by the app, from the project's files as it reads
//!   them ([`Store::offer`]); a request carries a target and a brief and nothing else;
//! - what a persona works with is read from records no chat writes: the vault registry, your
//!   own vault grants, and the project's file;
//! - a box is unticked until the person ticks it, and only a name the question offers **at
//!   the moment it is answered** is granted: the definition is read again then;
//! - the window sends back a digest of what it showed ([`DispatchPending::shown`]), and an
//!   answer to a question that reads differently now grants nothing. The person is shown it
//!   again.
//!
//! **A "this chat" grant ends with the chat** ([`Store::chat_closed`]), as its sandbox grants
//! do, and stays through a restart, which starts the new run before the old one ends
//! (D-1437-R3).
//!
//! # In one workspace, or in any (#1505)
//!
//! A grant carries one condition ([`purlis_core::dispatchwithin`]): it holds in any workspace,
//! or for a task that **works in** one workspace. Every entry point is told where the task
//! works ([`requested`]'s `works_in`, from the app's own record of the dispatch), and the
//! held dispatch keeps it.
//!
//! - **An Allow is the narrower grant** ([`Store::allow`], [`allow_dispatch`]): for the person
//!   and the project it holds in the held task's workspace. The wider one is a command of
//!   its own, [`allow_dispatch_anywhere`]. The window sends neither a pair nor a workspace.
//! - **A grant for one chat is for that task's place**: the chat is asked again when it sends
//!   the persona to work somewhere else.
//! - **One question a workspace**: a dispatch held for one workspace is not started by an
//!   Allow for another.
//! - **Settings changes it** ([`set_dispatch_workspace`]): narrowing and widening are each
//!   checked, recorded as the grant it was taken back and the grant it is made, then written
//!   once. A teammate's limited grant is accepted or declined for its workspace
//!   ([`accept_project_dispatch_in`], [`decline_project_dispatch_in`]).
//! - **A grant whose workspace is gone covers nothing**, and the table says why
//!   ([`DispatchGrant::nowhere`]).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use purlis_core::dispatchchain;
use purlis_core::dispatchgrant::{self, ChatPair, Covers, Dispatched, InForce, Pair};
use purlis_core::dispatchwants::{self, Access, Offer};
use purlis_core::dispatchwithin;
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
    /// Who is above it in its chain, as its own record keeps it (#1548): what the question
    /// reads to offer no box for a persona the person said never to for a chat above it.
    pub above: dispatchchain::Above,
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
        above: dispatchchain::Above::of(chat),
    }
}

/// Chat `session` as `chats` records it, in the project at `root`; none for a chat this app
/// does not have open.
#[cfg(test)]
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
    /// The workspace the task it asks for works in (#1505,
    /// [`purlis_core::dispatchwithin::works_in`]); none at the project's root. The app's own
    /// record of where the new chat will run, never a word of the request's.
    pub works_in: Option<String>,
}

/// One grant a person made for one chat, as Settings lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChatMade {
    pair: ChatPair,
    /// The workspace the dispatch it was allowed for works in; none at the project's root.
    /// The grant covers that chat's dispatches there, and nowhere else (#1505).
    works_in: Option<String>,
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
    /// Each of those chats' names, as its tab showed it when the person answered: what
    /// Settings says of whose a kept-blocked pair is (#1504).
    kept_names: Mutex<HashMap<Whose, String>>,
    /// The dispatches the person allowed while the record of nevers did not read, each for one
    /// start: no grant counts then, so their answer starts the one dispatch they read and no
    /// other ([`Store::allow`]). The same for a dispatch allowed into a workspace that is not
    /// there yet (#1505), for which no grant is kept at all.
    once: Mutex<Vec<Once>>,
}

/// **One start the person allowed, for one held dispatch**: the dispatch they read on the
/// Notice, and no other.
///
/// It is the held dispatch's own: its number, its chat, its pair, the workspace its task
/// works in, and **the brief the person was shown**. Asked again as it was first asked, that
/// dispatch is let through, and the pass is spent. An ask with any other brief is not it. The
/// pass ends when the answered dispatch returns, started or not ([`Store::end_once`]), and
/// goes when the pair's grant is revoked, said never to, or has its workspace changed
/// ([`Store::drop_once`]), so nothing the person took back is started by what is left of an
/// earlier yes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Once {
    /// The held dispatch's number.
    id: u32,
    whose: Whose,
    pair: ChatPair,
    works_in: Option<String>,
    brief: dispatchgrant::ShownBrief,
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

/// What the person ticked under an Allow, and what the question read as when they did.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ticked<'a> {
    /// The wanted personas whose boxes were ticked.
    pub also: &'a [String],
    /// [`DispatchPending::shown`] as the window was told it. `None` asks nothing of it: an
    /// answer with no question shown, which only a caller inside the app makes.
    pub shown: Option<&'a str>,
}

/// What an Allow is answered where the question no longer reads as it was shown.
pub const CHANGED: &str = "What this question says changed since it was shown, so nothing was \
                           allowed. Read it again, then answer.";

/// `names` as a sentence lists them: `qa`, `qa and docs`, `qa, docs and ops`.
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [most @ .., last] => format!("{} and {last}", most.join(", ")),
    }
}

impl Store {
    /// Hands every answer to `answers` from now on: the dispatch core's to register.
    pub fn answers_with(&self, answers: Answers) {
        *lock(&self.answers) = Some(answers);
    }

    /// The grants made for the chat whose id is `id`, for a task that works in `works_in`.
    fn chat_pairs(&self, id: Option<&str>, works_in: Option<&str>) -> Vec<ChatPair> {
        id.and_then(|id| lock(&self.chat).get(id).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|made| made.works_in.as_deref() == works_in)
            .map(|made| made.pair)
            .collect()
    }

    /// [`Store::in_force_for`], for a task at the project's root: no grant limited to a
    /// workspace counts, and of the chat's own only one made for a task at the root.
    pub fn in_force(&self, root: &Path, asking: &Asking) -> InForce {
        self.in_force_for(root, asking, None)
    }

    /// The grants in force for `asking` in the project at `root`, **for a task that works in
    /// `works_in`** (#1505; none: the project's root): what the dispatch decision reads to
    /// put a limit before a question to the person. Reading it grants nothing; the entry
    /// points below are still what a dispatch starts by.
    pub fn in_force_for(&self, root: &Path, asking: &Asking, works_in: Option<&str>) -> InForce {
        InForce::read(root, self.chat_pairs(asking.id.as_deref(), works_in)).for_task_in(works_in)
    }

    /// [`Store::request_in`], for a task at the project's root: what the tests of everything
    /// but the workspace condition ask.
    #[cfg(test)]
    pub fn request(
        &self,
        ground: &Ground<'_>,
        asking: Asking,
        target: &str,
        brief: &str,
        uncovered: Uncovered,
    ) -> (Requested, Option<Pending>) {
        self.request_in(ground, asking, target, brief, uncovered, None)
    }

    /// **A chat asked to dispatch to `target` with `brief`**: covered, held for the person, or
    /// locked. `asking` is the app's record of the chat. Asked twice for the same target and
    /// workspace while the first waits, it is the same held dispatch: one Notice, showing the
    /// first brief.
    /// With [`Uncovered::Refuse`] nothing is ever held: what is not covered is refused, in the
    /// one place a chat nobody is at is answered ([`crate::dispatchunattended::unattended`]),
    /// which reads no grant made for one chat. A chat that runs on another chat's grants is
    /// refused whatever it asks.
    ///
    /// **`works_in` is the workspace the task is to work in** (#1505), from the app's own
    /// record, none for the project's root. A grant limited to one workspace covers the
    /// dispatch only where that is the one; a dispatch held for the person is held for that
    /// workspace, and its Notice offers a grant for it.
    pub fn request_in(
        &self,
        ground: &Ground<'_>,
        asking: Asking,
        target: &str,
        brief: &str,
        uncovered: Uncovered,
        works_in: Option<&str>,
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
                    works_in,
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
        let grants = self.in_force_for(ground.root, &asking, works_in);
        match dispatchgrant::covers(asking.persona.as_deref(), target, &grants, ground.locks) {
            Covers::Covered => (
                Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat(target)),
                None,
            ),
            Covers::Locked(why) => {
                let told = self.hold(ground, asking, target, brief, Some(why.clone()), works_in);
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
            // The record of nevers does not read, so no grant counts, or the workspace the
            // task is to work in is not there yet, so no grant was kept; this one dispatch
            // the person allowed all the same, having read this very brief, and it starts
            // once. Any other brief is another dispatch, and is asked about.
            Covers::NeedsGrant | Covers::Unread
                if self.spend_once(&asking, target, works_in, brief) =>
            {
                (
                    Requested::Covered(dispatchgrant::grants_for_a_dispatched_chat(target)),
                    None,
                )
            }
            // Nothing covers it, or nothing counts until the record of nevers reads: the
            // person is asked ([`DispatchPending::never_unread`] says which).
            Covers::NeedsGrant | Covers::Unread => {
                match self.hold(ground, asking, target, brief, None, works_in) {
                    Ok((held, new)) => (
                        Requested::NeedsGrant { pending: held.id },
                        new.then_some(held),
                    ),
                    Err(why) => (Requested::Refused(why), None),
                }
            }
        }
    }

    /// Spends the one start the person allowed for the dispatch they read: `asking` to
    /// `target`, for a task in `works_in`, **with `brief` as they were shown it**. Answers
    /// whether there was one.
    fn spend_once(
        &self,
        asking: &Asking,
        target: &str,
        works_in: Option<&str>,
        brief: &str,
    ) -> bool {
        let mut once = lock(&self.once);
        let whose = Whose::of(asking);
        let shown = dispatchgrant::shown_brief(brief);
        let found = once.iter().position(|one| {
            one.whose == whose
                && one.pair.asking == asking.persona
                && one.pair.target == target
                && one.works_in.as_deref() == works_in
                && one.brief == shown
        });
        match found {
            Some(at) => {
                once.remove(at);
                true
            }
            None => false,
        }
    }

    /// **The answered dispatch `id` has returned**, started, held again or refused: what is
    /// left of its one start ends here, so it is never there for a later ask
    /// ([`crate::handoff::answered`] calls this on every arm).
    pub fn end_once(&self, id: u32) {
        lock(&self.once).retain(|one| one.id != id);
    }

    /// **The person took something back for `asking` to `target`** (`None`: every target, for
    /// "any persona"): a revoke, a never, or a change of a grant's workspace. Every one start
    /// still kept for the pair goes with it.
    fn drop_once(&self, asking: Option<&str>, target: Option<&str>) {
        lock(&self.once).retain(|one| {
            !(one.pair.asking.as_deref() == asking
                && target.is_none_or(|target| one.pair.target == target))
        });
    }

    /// **What the question for `held` says beyond who asks and the brief** (#1502): what the
    /// persona asked about works with, and each persona the asking one wants that the question
    /// offers beside it. Read from the project at `root` as it is now, and never from the
    /// request.
    ///
    /// A wanted persona is not offered where a grant covers it **for a task in the workspace
    /// this one works in** (#1505: a ticked pair is kept with the answer's own workspace
    /// condition, so a box is offered where that grant would be new), the person said never to
    /// it, or to it for a chat above this one in its chain (#1548), policy locks it, the person
    /// kept it blocked on this chat's tab (they are not asked twice in one chat, by a box
    /// either), or a dispatch to it from a chat of the same persona is waiting on the person:
    /// that one has its own question, which shows its own brief.
    ///
    /// **No box at all where the task's workspace is not there yet**: an Allow then starts the
    /// one dispatch and keeps no grant, so there is nothing a tick could be kept as.
    pub fn offer(&self, root: &Path, locks: &sandbox::policy::Locks, held: &Pending) -> Offer {
        let asking = held.asking.persona.as_deref();
        let missing = held
            .works_in
            .as_deref()
            .is_some_and(|workspace| !dispatchwithin::Seen::read(root).is_there(workspace));
        let wanted = if held.locked.is_some() || missing {
            Vec::new()
        } else {
            let grants = self.in_force_for(root, &held.asking, held.works_in.as_deref());
            dispatchwants::also(
                root,
                asking,
                &held.target,
                &grants,
                locks,
                &held.asking.above,
            )
        };
        let waiting = lock(&self.pending).clone();
        // A pair whose project grant the person said "Not on my machine" to is no box: a
        // tick at the project's level would accept it by another name. Settings is where
        // that is taken back.
        let declined = dispatchgrant::declined(root);
        let not_declined = |wanted: &str| {
            !asking.is_some_and(|asking| {
                dispatchgrant::project_grant_said(asking, wanted)
                    .is_some_and(|said| declined.contains(&said))
            })
        };
        Offer {
            target: Access::at(root, locks, &held.target),
            also: wanted
                .iter()
                .filter(|wanted| not_declined(wanted))
                .filter(|wanted| !self.is_kept_blocked(&held.asking, wanted))
                .filter(|wanted| {
                    !waiting.iter().any(|one| {
                        one.asking.persona.as_deref() == asking && one.target == **wanted
                    })
                })
                .map(|wanted| Access::at(root, locks, wanted))
                .collect(),
        }
    }

    /// [`Facts`] of `held`, with the other workspaces the asking chat's own grants allow the
    /// pair in.
    fn facts_of(&self, root: &Path, held: &Pending) -> Facts {
        let mut facts = facts(root, held);
        if held.locked.is_none() {
            facts.allowed_in.extend(self.chat_allowed_elsewhere(held));
            facts.allowed_in.sort();
            facts.allowed_in.dedup();
        }
        facts
    }

    /// `held` as the window is told it, in project `plane` at `root`: its [`Facts`], with what this
    /// store alone knows. The other workspaces the asking chat's own grants allow this pair
    /// in, so a chat allowed for one workspace that sends the persona to another is asked with
    /// the reason said (#1505); and what the question offers ([`Store::offer`], #1502).
    pub fn told(
        &self,
        plane: &PlaneId,
        root: &Path,
        locks: &sandbox::policy::Locks,
        held: &Pending,
    ) -> DispatchPending {
        let facts = self.facts_of(root, held);
        let offer = self.offer(root, locks, held);
        let stamp = stamp_of(&offer, &facts);
        let mut shown = told_of(plane, held, facts);
        shown.works_with = offer.target.said();
        shown.shown = stamp;
        shown.also = offer
            .also
            .iter()
            .map(|one| DispatchAlso {
                persona: one.persona.clone(),
                works_with: one.brief(),
            })
            .collect();
        shown
    }

    /// Holds a dispatch for the person, or answers the one already held for that chat,
    /// target and workspace, and whether it is new. **One question a workspace**: an answer
    /// that allows the pair in one workspace must not start a task waiting to run in another.
    fn hold(
        &self,
        ground: &Ground<'_>,
        asking: Asking,
        target: &str,
        brief: &str,
        locked: Option<String>,
        works_in: Option<&str>,
    ) -> Result<(Pending, bool), String> {
        let mut pending = lock(&self.pending);
        pending.retain(|one| (ground.is_open)(one.asking.session));
        if let Some(held) = pending.iter().find(|one| {
            one.asking.session == asking.session
                && one.target == target
                && one.works_in.as_deref() == works_in
        }) {
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
            works_in: works_in.map(str::to_owned),
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

    /// **Allow, at `level`, on the Notice of held dispatch `id`**, with no box ticked and no
    /// word of what was shown: [`Self::allow_with`], as the tests of everything else answer.
    /// The window's own Allow always says what it showed.
    #[cfg(test)]
    pub fn allow(&self, ground: &Ground<'_>, id: u32, level: Level) -> Result<String, String> {
        self.allow_with(ground, id, level, &Ticked::default())
    }

    /// **Allow, at `level`, in any workspace**, with no box ticked and no word of what was
    /// shown: [`Self::allow_anywhere_with`], as the tests of the workspace condition answer.
    #[cfg(test)]
    pub fn allow_anywhere(
        &self,
        ground: &Ground<'_>,
        id: u32,
        level: Level,
    ) -> Result<String, String> {
        self.allow_anywhere_with(ground, id, level, &Ticked::default())
    }

    /// **Allow, at `level`, on the Notice of held dispatch `id`, with `ticked`** (#1502):
    /// judged again under policy, audited, then kept where that level keeps it. The asked pair
    /// and every ticked one are kept at that level, **each audited as its own grant**. The
    /// held dispatch, and every other one a grant now covers, is handed on to start.
    ///
    /// **The grant is the narrower one** (#1505): for the person or the project it holds in
    /// the workspace the held task works in, and nowhere else. [`Store::allow_anywhere_with`]
    /// is the wider answer, and a press of its own. A task at the project's root works in no
    /// workspace, so there the grant holds in any, as it always did. **Every ticked pair is
    /// kept with the same condition as the asked one**: one answer, one place it holds.
    ///
    /// **Only what the question offers now is granted.** The asking persona's definition is
    /// read again here: a ticked name it no longer offers is not granted, and the answer says
    /// so. Where the window says what it showed ([`Ticked::shown`]) and the question reads
    /// differently now, nothing at all is granted or audited ([`CHANGED`]).
    ///
    /// The asked pair comes first: if it cannot be kept, nothing else is tried. A ticked pair
    /// that then fails to be written is recorded as taken back and said, and the rest stand.
    pub fn allow_with(
        &self,
        ground: &Ground<'_>,
        id: u32,
        level: Level,
        ticked: &Ticked<'_>,
    ) -> Result<String, String> {
        self.allow_within(ground, id, level, false, ticked)
    }

    /// **Allow, at `level`, in any workspace, with `ticked`**: the Notice's explicit wider
    /// choice, for the asked pair and for every ticked one alike. For one chat it is
    /// [`Store::allow_with`]: a grant for one chat is for the task it was asked about.
    pub fn allow_anywhere_with(
        &self,
        ground: &Ground<'_>,
        id: u32,
        level: Level,
        ticked: &Ticked<'_>,
    ) -> Result<String, String> {
        self.allow_within(ground, id, level, true, ticked)
    }

    /// Both Allows.
    fn allow_within(
        &self,
        ground: &Ground<'_>,
        id: u32,
        level: Level,
        anywhere: bool,
        ticked: &Ticked<'_>,
    ) -> Result<String, String> {
        use purlis_core::dispatchwithin::{Limited, Within};
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
        // The question as it reads now, from the files as they are now: what the person saw
        // is held to it, and so is every box. **All of what it said**: the access lines and
        // the boxes, the answers offered, where it is already allowed, whether the list of
        // nevers reads, and whether the workspace is there.
        let offer = self.offer(ground.root, ground.locks, &held);
        let now = self.facts_of(ground.root, &held);
        if ticked
            .shown
            .is_some_and(|shown| shown != stamp_of(&offer, &now))
        {
            return Err(CHANGED.to_owned());
        }
        let mut wanted: Vec<&str> = Vec::new();
        let mut not_offered: Vec<String> = Vec::new();
        for name in ticked.also {
            if offer.also.iter().any(|one| one.persona == *name) {
                if !wanted.contains(&name.as_str()) {
                    wanted.push(name);
                }
            } else {
                let name = purlis_core::shown::short(name);
                if !not_offered.contains(&name) {
                    not_offered.push(name);
                }
            }
        }
        // What is kept, checked before anything is audited.
        enum Kept {
            Chat(String, ChatPair),
            Pair(Pair),
        }
        let kept_for = |target: &str| match (level, asking, held.asking.id.as_deref()) {
            (Level::Chat, _, Some(chat)) => Ok(Kept::Chat(
                chat.to_owned(),
                ChatPair {
                    asking: held.asking.persona.clone(),
                    target: target.to_owned(),
                },
            )),
            (Level::Chat, _, None) => {
                Err("That chat has no id yet, so purlis can allow nothing for it alone.".to_owned())
            }
            (Level::You | Level::Project, Some(asking), _) => {
                Pair::new(asking, target).map(Kept::Pair)
            }
            (Level::You | Level::Project, None, _) => Err(
                "This chat runs as no persona, so there is no pair to allow beyond this chat. \
                 Allow it for this chat."
                    .to_owned(),
            ),
        };
        let asked = kept_for(&held.target)?;
        // **Only an answer the question offers is taken**, whatever the window sends: not the
        // project's level for a pair the person said "Not on my machine" to, nor any level
        // where policy locks it. Where the workspace is not there yet the question offers one
        // answer under one name, and whichever is pressed keeps nothing (below).
        let offered = match level {
            Level::Chat => GrantLevel::Chat,
            Level::You => GrantLevel::You,
            Level::Project => GrantLevel::Project,
        };
        if !now.missing && !now.levels.contains(&offered) {
            return Err(
                "That answer is not one this question offers, so nothing was allowed. Read it \
                 again, then answer."
                    .to_owned(),
            );
        }
        let also: Vec<(&str, Kept)> = wanted
            .iter()
            .map(|target| kept_for(target).map(|kept| (*target, kept)))
            .collect::<Result<_, _>>()?;
        // What the answer says of the boxes, after what it says of the asked pair: `held_in`
        // is where the ticked pairs hold, as the asked one does.
        let of_the_boxes = |kept_too: &[String], unkept: &[String], held_in: &str| {
            let mut said = String::new();
            if !kept_too.is_empty() {
                said.push_str(&format!(
                    " Also allowed {}{held_in}: {} to {}.",
                    level.said(),
                    asking.unwrap_or("this chat"),
                    listed(kept_too)
                ));
            }
            if !not_offered.is_empty() {
                said.push_str(&format!(
                    " Not allowed, since the question no longer offers it: {}.",
                    listed(&not_offered)
                ));
            }
            if !unkept.is_empty() {
                said.push_str(&format!(" Not kept: {}.", unkept.join("; ")));
            }
            said
        };
        // **The workspace the task is to work in is not there yet** (a handoff that makes it):
        // no grant is kept, at any level, for a name that is no workspace, since it would
        // belong to whatever was made under that name next. The person read this dispatch
        // and allowed it: it starts, once, recorded as that, and the next one asks. No box is
        // offered there ([`Store::offer`]), so no ticked pair is kept either.
        if let Some(workspace) = held.works_in.as_deref()
            && !dispatchwithin::Seen::read(ground.root).is_there(workspace)
        {
            (ground.audit)(
                Some(held.asking.session),
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::Once,
                    asking,
                    target: &held.target,
                    level,
                    workspace: Some(workspace),
                },
            )?;
            if let Some(pending) = self.take(id) {
                self.start_once(pending);
            }
            return Ok(format!(
                "{} is not a workspace of this project yet, so this starts this one dispatch \
                 and keeps no grant. The next one asks you.{}",
                purlis_core::shown::short(workspace),
                of_the_boxes(&[], &[], "")
            ));
        }
        // **Where it holds**, for the asked pair and every ticked one alike. For the person
        // and the project: the workspace the held task works in, unless they chose any
        // workspace. For one chat: that task's place, always.
        let within = match (&asked, held.works_in.as_deref()) {
            (Kept::Pair(_), Some(workspace)) if !anywhere => {
                Within::Workspace(workspace.to_owned())
            }
            _ => Within::Any,
        };
        let limited_of = |kept: &Kept| match (kept, &within) {
            (Kept::Pair(pair), Within::Workspace(workspace)) => {
                Limited::of(pair, workspace).map(Some)
            }
            _ => Ok(None),
        };
        // Whether each can be written is asked before any is recorded, so the log holds no
        // grant that was refused.
        for kept in std::iter::once(&asked).chain(also.iter().map(|(_, kept)| kept)) {
            if let (Level::Project, Kept::Pair(pair)) = (level, kept) {
                match limited_of(kept)? {
                    Some(one) => purlis_core::settings::dispatch::can_grant_in(ground.root, &one)?,
                    None => purlis_core::settings::dispatch::can_grant(ground.root, pair)?,
                }
            }
        }
        // One pair: audited as its own grant, with where it holds, then kept.
        let keep = |target: &str, kept: &Kept| -> Result<(), String> {
            let limited = limited_of(kept)?;
            let audited = dispatchgrant::Audited {
                act: dispatchgrant::Act::Grant,
                asking,
                target,
                level,
                workspace: match kept {
                    Kept::Chat(..) => held.works_in.as_deref(),
                    Kept::Pair(_) => within.workspace(),
                },
            };
            (ground.audit)(Some(held.asking.session), &audited)?;
            // A write that fails all the same is recorded as taken back, so the log never ends
            // on a grant that is not there.
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
            match kept {
                Kept::Chat(chat, pair) => {
                    let mut made = lock(&self.chat);
                    let mine = made.entry(chat.clone()).or_default();
                    if !mine
                        .iter()
                        .any(|one| one.pair == *pair && one.works_in == held.works_in)
                    {
                        mine.push(ChatMade {
                            pair: pair.clone(),
                            works_in: held.works_in.clone(),
                            at: ground.at,
                            chat: held.asking.name.clone(),
                        });
                    }
                }
                Kept::Pair(pair) => {
                    match (level, &limited) {
                        (Level::Project, Some(one)) => {
                            purlis_core::settings::dispatch::write_in(ground.root, one)
                                .map_err(not_kept)?;
                            // The file holds it now. Where this machine cannot record its own
                            // acceptance, that is said, and nothing is recorded as taken back:
                            // the grant is in the file, for the team.
                            dispatchwithin::accept(ground.root, one).map_err(|why| {
                                purlis_core::settings::dispatch::written_not_accepted(&why)
                            })?;
                        }
                        (Level::Project, None) => {
                            purlis_core::settings::dispatch::grant(ground.root, pair)
                                .map_err(not_kept)?;
                        }
                        _ => dispatchwithin::grant_yours(ground.root, pair, &within).map_err(
                            |why| not_kept(format!("purlis could not keep the grant: {why}")),
                        )?,
                    }
                    if let Err(why) = sandbox::local::record_made(
                        ground.root,
                        sandbox::local::Made {
                            what: WHAT.to_owned(),
                            target: limited
                                .as_ref()
                                .map_or_else(|| pair.to_string(), ToString::to_string),
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
            Ok(())
        };
        keep(&held.target, &asked)?;
        let mut kept_too: Vec<String> = Vec::new();
        let mut unkept: Vec<String> = Vec::new();
        for (target, kept) in &also {
            match keep(target, kept) {
                Ok(()) => kept_too.push((*target).to_owned()),
                Err(why) => unkept.push(format!("{target} ({why})")),
            }
        }
        // Where the ticked pairs hold, said as the asked one's is.
        let held_in = match &asked {
            Kept::Pair(_) => format!(", {}", within.said()),
            Kept::Chat(..) => String::new(),
        };
        let boxes = of_the_boxes(&kept_too, &unkept, &held_in);
        self.start_what_is_covered(ground);
        // The record of nevers does not read, so the grant just kept covers nothing yet. The
        // person read that on the Notice and allowed this dispatch: it starts, once, and the
        // next one asks again until the record reads. No box is offered then
        // ([`Store::offer`]), so nothing else was kept.
        if self.in_force(ground.root, &held.asking).never_unread {
            if let Some(pending) = self.take(id) {
                self.start_once(pending);
            }
            return Ok(format!(
                "Allowed {}. This dispatch starts now. The next one asks again until the list \
                 of pairs you said never to reads.{boxes}",
                level.said()
            ));
        }
        // Still held: the grant is kept and does not cover this dispatch (its workspace went
        // away this moment, or the workspaces cannot be looked at). Nothing is started on
        // the strength of a grant that does not cover it: the question stays.
        if lock(&self.pending).iter().any(|one| one.id == id) {
            return Err(format!(
                "The grant is kept, and it does not cover this dispatch just now, so nothing \
                 was started and it still waits. Answer it again, or look at the grant in {}.\
                 {boxes}",
                dispatchgrant::SETTINGS
            ));
        }
        let from_this_chat = match held.works_in.as_deref() {
            Some(workspace) => format!("works in {}", purlis_core::shown::short(workspace)),
            None => "works at the project's root".to_owned(),
        };
        let said = match (&asked, &within) {
            (Kept::Pair(_), Within::Workspace(workspace)) => format!(
                "Allowed {}, in {workspace}. The dispatch starts now, and the next one that \
                 works in {workspace} starts without asking.",
                level.said(),
                workspace = purlis_core::shown::short(workspace)
            ),
            // At the project's root there is no narrower grant, and it is said as what it is.
            (Kept::Pair(_), Within::Any) => format!(
                "Allowed {}, in any workspace. The dispatch starts now, and the next one \
                 starts without asking.",
                level.said()
            ),
            (Kept::Chat(..), _) => format!(
                "Allowed {}. The dispatch starts now, and the next one from this chat that \
                 {from_this_chat} starts without asking.",
                level.said()
            ),
        };
        Ok(format!("{said}{boxes}"))
    }

    /// Starts `pending`, which the person allowed and no grant can cover just now: one start
    /// for that dispatch and its brief ([`Once`]), spent when it is asked again and ended when
    /// it returns.
    fn start_once(&self, pending: Pending) {
        lock(&self.once).push(Once {
            id: pending.id,
            whose: Whose::of(&pending.asking),
            pair: ChatPair {
                asking: pending.asking.persona.clone(),
                target: pending.target.clone(),
            },
            works_in: pending.works_in.clone(),
            brief: pending.brief.clone(),
        });
        self.answer(&Answered {
            allowed: Some(dispatchgrant::grants_for_a_dispatched_chat(&pending.target)),
            pending,
        });
    }

    /// The workspaces, other than the one `held`'s task works in, that the asking chat's own
    /// grants allow the pair in.
    fn chat_allowed_elsewhere(&self, held: &Pending) -> Vec<String> {
        let Some(id) = held.asking.id.as_deref() else {
            return Vec::new();
        };
        lock(&self.chat)
            .get(id)
            .into_iter()
            .flatten()
            .filter(|made| {
                made.pair.asking == held.asking.persona
                    && made.pair.target == held.target
                    && made.works_in != held.works_in
            })
            .filter_map(|made| made.works_in.clone())
            .collect()
    }

    /// Hands on every held dispatch a grant now covers.
    fn start_what_is_covered(&self, ground: &Ground<'_>) {
        let waiting: Vec<Pending> = lock(&self.pending).clone();
        for held in waiting {
            let grants = self.in_force_for(ground.root, &held.asking, held.works_in.as_deref());
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
        let mut names = lock(&self.kept_names);
        kept.remove(&Whose::Session(session));
        names.remove(&Whose::Session(session));
        once.retain(|one| one.whose != Whose::Session(session));
        if let Some(id) = id {
            lock(&self.chat).remove(id);
            kept.remove(&Whose::Id(id.to_owned()));
            names.remove(&Whose::Id(id.to_owned()));
            once.retain(|one| one.whose != Whose::Id(id.to_owned()));
        }
    }

    /// **Every pair kept blocked for one chat's life**, with the chat's name as its tab showed
    /// it: Settings lists them, read-only (#1504). Each ends with its chat; nothing here is
    /// kept on disk. Sorted by chat, then by pair.
    pub fn kept_blocked(&self) -> Vec<DispatchKeptBlocked> {
        let names = lock(&self.kept_names);
        let mut out: Vec<DispatchKeptBlocked> = lock(&self.kept_blocked)
            .iter()
            .flat_map(|(whose, pairs)| {
                let chat = names.get(whose).cloned().unwrap_or_default();
                pairs.iter().map(move |pair| DispatchKeptBlocked {
                    chat: chat.clone(),
                    asking: pair.asking.clone(),
                    target: pair.target.clone(),
                })
            })
            .collect();
        out.sort_by(|a, b| (&a.chat, &a.asking, &a.target).cmp(&(&b.chat, &b.asking, &b.target)));
        out
    }

    /// **The name `name` changed hands** (#1504): purlis removed the persona or is about to
    /// make one under the name, or it was seen gone and is another persona's now. Every grant
    /// made for one chat that names it ends, as asking persona or as target. Answers how many
    /// ended.
    pub fn persona_gone(&self, name: &str) -> usize {
        let mut ended = 0;
        for made in lock(&self.chat).values_mut() {
            let before = made.len();
            made.retain(|one| one.pair.asking.as_deref() != Some(name) && one.pair.target != name);
            ended += before - made.len();
        }
        ended
    }

    /// Every persona's name a grant made for one chat holds: what the core is told beside
    /// this machine's own records, so a name only such a grant holds is seen gone too.
    fn chat_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for one in lock(&self.chat).values().flatten() {
            for name in one
                .pair
                .asking
                .iter()
                .chain(std::iter::once(&one.pair.target))
            {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
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

    /// **The person accepted `shown`, pairs of the project's** (D-1437-R1): each pair of it
    /// the committed file holds and this machine had not acknowledged is audited as a grant
    /// for everyone, then in force here, and every held dispatch it covers is handed on. A
    /// pair the file does not hold is nothing.
    pub fn acknowledge(&self, ground: &Ground<'_>, shown: &[String]) -> Result<(), String> {
        for pair in dispatchgrant::unacknowledged(ground.root) {
            if !shown.contains(&pair.to_string()) {
                continue;
            }
            let audited = dispatchgrant::Audited {
                act: dispatchgrant::Act::Grant,
                asking: Some(&pair.asking),
                target: &pair.target,
                level: Level::Project,
                workspace: None,
            };
            (ground.audit)(None, &audited)?;
            dispatchgrant::acknowledge_pair(ground.root, &pair).map_err(|why| {
                // Recorded as taken back, so the log never ends on an acceptance that is
                // not there (the project's history could not be asked, or the file moved).
                if let Err(unsaid) = (ground.audit)(
                    None,
                    &dispatchgrant::Audited {
                        act: dispatchgrant::Act::Revoke,
                        ..audited
                    },
                ) {
                    tracing::warn!(
                        "purlis: an acceptance that was not kept is still recorded as made \
                         ({unsaid})"
                    );
                }
                format!("purlis could not record it as allowed: {why}")
            })?;
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
                lock(&self.kept_names)
                    .insert(Whose::of(&pending.asking), pending.asking.name.clone());
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
            workspace: None,
        };
        (ground.audit)(Some(held.asking.session), &audited)?;
        // When, and on which chat's question (#1464): what Settings' table says of it.
        let said = purlis_core::dispatchnever::Said {
            at: Some(ground.at),
            chat: Some(held.asking.name.clone()),
        };
        if let Err(why) = dispatchgrant::never_said_as(ground.root, &pair, said) {
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
        self.drop_once(Some(asking), Some(&held.target));
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

    /// Takes back chat `id`'s grant of `pair` for tasks in `works_in`. Answers whether there
    /// was one.
    fn revoke_chat(&self, id: &str, pair: &ChatPair, works_in: Option<&str>) -> bool {
        let mut made = lock(&self.chat);
        let Some(mine) = made.get_mut(id) else {
            return false;
        };
        let before = mine.len();
        mine.retain(|one| !(one.pair == *pair && one.works_in.as_deref() == works_in));
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
    /// The workspace the task works in (#1505); null at the project's root. Where it is one,
    /// an Allow for the person or the project holds in it alone unless the person chooses
    /// any workspace, which is a command of its own ([`allow_dispatch_anywhere`]). Where it
    /// is null and an Allow for the person or the project is offered, that Allow holds in
    /// any workspace, and the Notice says so.
    pub works_in: Option<String>,
    /// Whether `works_in` is not a workspace of the project yet (a handoff that makes it):
    /// no grant is kept for a name that is no workspace, so an Allow starts this one dispatch
    /// and the next one asks. One Allow is offered, and no choice of where it holds.
    pub works_in_missing: bool,
    /// The other workspaces this dispatch is already allowed in, sorted: by a grant for this
    /// chat, the person's, or the project's. Where there are any, the Notice says why the
    /// person is asked again: the grant they made holds there, and this task works elsewhere.
    pub allowed_in: Vec<String>,
    /// **What the target persona works with**, in purlis's words (#1502): its vaults' names,
    /// the hosts the project declares for it and the personas it may itself dispatch to, each
    /// clipped to a few; or that no list holds it, where the project's sandbox is off. Never a
    /// secret's name or value, and nothing a chat wrote.
    pub works_with: String,
    /// The personas the asking persona's definition wants that nothing answers for yet: one
    /// box each under the answers, unticked. Read from the definition by the app as it tells
    /// this, never from the request. Empty where policy locks the dispatch.
    pub also: Vec<DispatchAlso>,
    /// A digest of `works_with` and `also`, the part a long list clips included: an Allow
    /// sends it back, and one for a question that reads differently now grants nothing.
    pub shown: String,
}

/// What a dispatch's question draws its answers from, read from the project at `root`: the
/// answers offered, whether the task's workspace is there yet, whether the list of nevers
/// reads, and the workspaces the pair is already allowed in.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Facts {
    levels: Vec<GrantLevel>,
    missing: bool,
    never_unread: Option<String>,
    allowed_in: Vec<String>,
}

/// `held` as the window is told it, in project `plane`, **less what the store says of it**:
/// what the question offers and what the target works with are [`Store::told`]'s to fill, and
/// are empty here. The tests' reading of it; the window is told by [`Store::told`].
#[cfg(test)]
fn told(plane: &PlaneId, root: &Path, held: &Pending) -> DispatchPending {
    told_of(plane, held, facts(root, held))
}

/// [`Facts`] of `held` in the project at `root`.
fn facts(root: &Path, held: &Pending) -> Facts {
    let mut levels = Vec::new();
    if held.locked.is_none() {
        if held.asking.id.is_some() {
            levels.push(GrantLevel::Chat);
        }
        if held.asking.persona.is_some() {
            levels.push(GrantLevel::You);
            // Not for a pair the person said "Not on my machine" to: Settings is where that
            // is taken back, and this question does not undo it by another name.
            let declined = held.asking.persona.as_deref().is_some_and(|asking| {
                dispatchgrant::project_grant_said(asking, &held.target)
                    .is_some_and(|said| dispatchgrant::declined(root).contains(&said))
            });
            if purlis_core::names::has_manifest(root) && !declined {
                levels.push(GrantLevel::Project);
            }
        }
    }
    // No grant is kept for a workspace that is not there yet, so there is one answer to give.
    let missing = held.locked.is_none()
        && held
            .works_in
            .as_deref()
            .is_some_and(|workspace| !dispatchwithin::Seen::read(root).is_there(workspace));
    if missing {
        levels.truncate(1);
    }
    // Where the person's and the project's limited grants already allow the pair: as they
    // are in force, by the last settling's verdict (#1506), never as kept on disk.
    let mut allowed_in: Vec<String> = match (&held.locked, held.asking.persona.as_deref()) {
        (None, Some(asking)) => InForce::read(root, Vec::new())
            .limited
            .into_iter()
            .filter(|(_, one)| {
                one.asking == asking
                    && (one.target == held.target || one.any())
                    && Some(one.workspace.as_str()) != held.works_in.as_deref()
            })
            .map(|(_, one)| one.workspace)
            .collect(),
        _ => Vec::new(),
    };
    allowed_in.sort();
    allowed_in.dedup();
    Facts {
        levels,
        missing,
        never_unread: held
            .locked
            .is_none()
            .then(|| dispatchgrant::nevers_unread(root))
            .flatten(),
        allowed_in,
    }
}

/// `held` as the window is told it, from its [`Facts`].
fn told_of(plane: &PlaneId, held: &Pending, facts: Facts) -> DispatchPending {
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
        levels: facts.levels,
        never_unread: facts.never_unread,
        locked: held.locked.clone(),
        works_in: held.works_in.clone(),
        works_in_missing: facts.missing,
        allowed_in: facts.allowed_in,
        works_with: String::new(),
        also: Vec::new(),
        shown: String::new(),
    }
}

/// **The digest of everything a dispatch's question says beyond who asks and the brief**: what
/// the target and each box's persona work with ([`Offer::stamp`]), and the facts the Notice
/// draws its answers from: the answers offered, whether the workspace is there yet, whether
/// the list of nevers reads, and where the pair is already allowed. An Allow sends it back,
/// and one for a question that reads differently now grants nothing.
fn stamp_of(offer: &Offer, told: &Facts) -> String {
    let levels: Vec<&str> = told
        .levels
        .iter()
        .map(|level| match level {
            GrantLevel::Chat => "chat",
            GrantLevel::You => "you",
            GrantLevel::Project => "project",
        })
        .collect();
    format!(
        "{}\u{1e}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        offer.stamp(),
        levels.join(","),
        told.missing,
        told.never_unread.as_deref().unwrap_or_default(),
        told.allowed_in.join(",")
    )
}

/// One persona a dispatch's Notice offers beside the one asked about (#1502).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchAlso {
    pub persona: String,
    /// What it works with, as `works_with` says it of the target, without the persona's name.
    pub works_with: String,
}

/// [`Store::told`], under this machine's policy as it is now: what the tests of the workspace
/// condition read.
#[cfg(test)]
fn told_by(store: &Store, plane: &PlaneId, root: &Path, held: &Pending) -> DispatchPending {
    store.told(plane, root, &sandbox::policy::Locks::of(root), held)
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
#[cfg(test)]
pub fn request_dispatch_grant(
    held: &crate::planes::Held,
    session: u32,
    target: &str,
    brief: &str,
    works_in: Option<&str>,
) -> Requested {
    requested(
        held,
        session,
        target,
        brief,
        Uncovered::AskThePerson,
        works_in,
    )
}

/// **THE DISPATCH CORE'S ENTRY POINT, for a chat nobody is at (an unattended chat, #1446).**
///
/// [`request_dispatch_grant`], but a dispatch nothing covers is a plain refusal: nothing is
/// held, no Notice is raised, and a locked one raises none either. So an unattended chat
/// dispatches only under a grant that already exists and that this machine has acknowledged,
/// and a missing grant is a sentence for the chat, never a prompt nobody is there to answer.
#[cfg(test)]
pub fn request_dispatch_grant_or_refuse(
    held: &crate::planes::Held,
    session: u32,
    target: &str,
    brief: &str,
    works_in: Option<&str>,
) -> Requested {
    requested(held, session, target, brief, Uncovered::Refuse, works_in)
}

/// What a chat is told when it asks to dispatch while the persona it runs as is no persona
/// of the project.
pub fn gone_persona_said(persona: &str) -> String {
    let persona = purlis_core::shown::short(persona);
    format!(
        "this chat runs as {persona}, which is not a persona of this project now, so no \
         dispatch grant counts for it and it dispatches to no one. Nothing was started and \
         the person was not asked. Tell the person: they make the persona again, or start a \
         chat as another."
    )
}

/// **Brings the records of dispatch up to what the project's personas are now**, before a
/// dispatch is judged ([`purlis_core::dispatchdormant::judged`]): a name seen gone is marked,
/// and a marked name that is another persona's now has its grants set aside. Each grant set
/// aside is recorded in the event log as taken back by purlis, and the grants made for one
/// chat that name it end. A record that could not be written is said in the log and stops
/// nothing: what is not written is not in force either, while the name is no persona.
fn bring_up_to_date(root: &Path, store: &Store, audit: Audit<'_>) {
    let judged = match purlis_core::dispatchdormant::judged(root, &store.chat_names()) {
        Ok(judged) => judged,
        Err(why) => {
            tracing::warn!("purlis: the records of dispatch were not brought up to date ({why})");
            return;
        }
    };
    record_set_aside(&judged.aside, audit);
    for name in &judged.changed_hands {
        store.persona_gone(name);
    }
}

/// Records each of `aside` in the event log as a grant purlis took back
/// ([`dispatchgrant::Act::SetAside`]). Best effort: they are out of force already.
fn record_set_aside(aside: &purlis_core::dispatchdormant::SetAside, audit: Audit<'_>) {
    let grants = aside
        .grants
        .iter()
        .map(|one| (one.asking.as_str(), one.target.as_str(), Level::You));
    let accepted = aside.accepted.iter().filter_map(|one| {
        let (asking, target) = one.said.split_once(" -> ")?;
        Some((asking, target, Level::Project))
    });
    let whole = grants
        .chain(accepted)
        .map(|(asking, target, level)| (asking, target, level, None));
    // A grant limited to one workspace ended with the name: said the same way, with where
    // it held.
    let ended = aside.ended.iter().map(|(level, one)| {
        (
            one.asking.as_str(),
            one.target.as_str(),
            *level,
            Some(one.workspace.as_str()),
        )
    });
    for (asking, target, level, workspace) in whole.chain(ended) {
        if let Err(unsaid) = audit(
            None,
            &dispatchgrant::Audited {
                act: dispatchgrant::Act::SetAside,
                asking: Some(asking),
                target,
                level,
                workspace,
            },
        ) {
            tracing::warn!("purlis: a dispatch grant set aside is not in the event log ({unsaid})");
        }
    }
}

/// Both entry points. `works_in` is the workspace the task is to work in (#1505,
/// [`purlis_core::dispatchwithin::works_in`]), none for the project's root: **every caller
/// says it**, so no dispatch is judged as if a grant limited to one workspace held in all.
#[cfg(test)]
fn requested(
    held: &crate::planes::Held,
    session: u32,
    target: &str,
    brief: &str,
    uncovered: Uncovered,
    works_in: Option<&str>,
) -> Requested {
    let Some(asking) = asking_of(held.chats(), held.root(), session) else {
        return Requested::Refused(format!("chat {session} is not one this app has open"));
    };
    requested_as(held, asking, target, brief, uncovered, works_in)
}

/// [`requested`], for the asking chat as the caller already read it: so what the caller
/// decided by and what this answers by are one read of who the chat is (#1543).
pub fn requested_as(
    held: &crate::planes::Held,
    asking: Asking,
    target: &str,
    brief: &str,
    uncovered: Uncovered,
    works_in: Option<&str>,
) -> Requested {
    let root = held.root();
    let audit: Audit<'_> =
        &|number, audited| held.hooks().record_dispatch_grant(root, number, audited);
    // Before any grant is read: the records are brought up to what the project's personas
    // are now, so nothing an earlier persona of a name was allowed covers this dispatch.
    bring_up_to_date(root, held.dispatch_grants(), audit);
    let personas = purlis_core::dispatchdormant::personas_of(root);
    let is_persona = |name: &str| {
        personas
            .as_ref()
            .is_some_and(|all| all.iter().any(|one| one == name))
    };
    // **A grant is in force only while both personas exist** (#1504). A chat that still runs
    // as a persona the project no longer has is covered by nothing, whatever is granted its
    // name: not a named pair, not "any persona", not the project's. A chat that runs on
    // another chat's grants is refused with its own sentence below.
    if !asking.held
        && let Some(persona) = asking.persona.as_deref()
        && !is_persona(persona)
    {
        return Requested::Refused(gone_persona_said(persona));
    }
    if !is_persona(target) {
        return Requested::Refused(format!(
            "this project has no persona named {}, so there is nothing to dispatch to.",
            purlis_core::shown::short(target)
        ));
    }
    let locks = sandbox::policy::Locks::of(root);
    let (answer, raised) = held.dispatch_grants().request_in(
        &Ground {
            root,
            locks: &locks,
            is_open: &|session| held.chats().recorded_chat(session).is_some(),
            sandboxed: &|session| held.chats().confines_of(session).is_some(),
            audit,
            at: now_secs(),
        },
        asking,
        target,
        brief,
        uncovered,
        works_in,
    );
    if let (Some(raised), Some(tell)) = (raised, TELL.get()) {
        tell(
            held.dispatch_grants()
                .told(held.plane_id(), root, &locks, &raised),
        );
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
    let root = held.root();
    let locks = sandbox::policy::Locks::of(root);
    let store = held.dispatch_grants();
    Ok(store
        .waiting(session)
        .iter()
        .map(|one| store.told(&plane, root, &locks, one))
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
///
/// `also` is the wanted personas whose boxes the person ticked (#1502): each the question
/// still offers is kept at the same level and with the same workspace condition as the asked
/// pair (#1505), as its own audited grant. `shown` is the digest the
/// Notice was told; where the question reads differently now, nothing is allowed and the
/// Notice reads it again.
// On a blocking thread ([`SETTLES`]): an Allow for everyone ends in this machine's acceptance
// of the project's grant, which settles against git's history.
#[tauri::command]
#[specta::specta]
pub async fn allow_dispatch(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: u32,
    level: GrantLevel,
    also: Vec<String>,
    shown: String,
) -> Result<DispatchAllowed, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("allowing the dispatch", move || {
        let allowed = with_ground(&held, |ground| {
            held.dispatch_grants().allow_with(
                ground,
                id,
                level.into(),
                &Ticked {
                    also: &also,
                    shown: Some(&shown),
                },
            )
        })
        .map(|said| DispatchAllowed { said });
        // An Allow for everyone answers a grant of the project's that was waiting, where it
        // was.
        arrival_moved(&plane);
        allowed
    })
    .await
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

/// **The window commands that may settle this machine's acceptances against git's history**
/// (#1506): the three of the arrival Notice, and every command that can end in an acceptance
/// of a grant of the project's. Each is `async` and does its work on a blocking thread, so a
/// slow history never holds the thread that draws the window; each is bounded by the
/// settling's own deadline ([`purlis_core::dispatcharrival`]) and takes no lock a dispatch is
/// decided under. `off_the_main_thread`'s test asks each and holds them to it, which is the
/// list's one reader.
#[cfg(test)]
pub const SETTLES: [&str; 10] = [
    "dispatch_arrival",
    "answer_dispatch_arrival",
    "dispatch_gone_told",
    "allow_dispatch",
    "allow_dispatch_anywhere",
    "accept_project_dispatch",
    "allow_dispatch_to_any",
    "set_dispatch_workspace",
    "accept_project_dispatch_in",
    "give_back_dispatch",
];

/// **The window commands that ask git's history without settling anything** (#1543): Settings'
/// list of grants names who committed each of the project's, and asks git once a page (#1464). Each
/// is `async` and asks on a blocking thread, as [`SETTLES`] do. `off_the_main_thread`'s test
/// holds them to it.
#[cfg(test)]
pub const READS_HISTORY: [&str; 2] = ["dispatch_grants", "revoke_dispatch_grant"];

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
    /// When the person said it, in seconds since 1970; null where that is not known (#1464).
    pub at: Option<u32>,
    /// The asking chat's name, as its tab showed it, where it was said on a chat's question;
    /// null where it was said on none, or that is not known.
    pub chat: Option<String>,
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
    /// Whether it is the project's and the person said "Not on my machine" to it (#1504): it
    /// covers nothing here, and is not told as new, until it is accepted in Settings.
    pub declined: bool,
    /// The workspace it is limited to (#1505); null where it holds in any workspace.
    pub workspace: Option<String>,
    /// Why it covers nothing because of its workspace, where it does not: the workspace is
    /// gone, or one was made again under its name since. Null where its workspace stands.
    pub nowhere: Option<String>,
    /// What Clear is sent by ([`revoke_dispatch_grant`]) for one limited to a workspace; null
    /// for one that holds in any, which is cleared by its persona and level.
    pub id: Option<String>,
}

/// A pair one chat was kept blocked for, as Settings lists it: read-only, and gone when the
/// chat closes (#1504).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchKeptBlocked {
    /// The chat's name, as its tab showed it when the person answered.
    pub chat: String,
    /// The persona the chat runs as; null for a chat on no persona.
    pub asking: Option<String>,
    pub target: String,
}

/// A grant of yours set aside because a persona it named was no longer the project's
/// (#1504): in force for no chat until the person gives it back or removes it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchDormant {
    pub asking: String,
    /// The target persona's name, or `*` where `any`.
    pub target: String,
    /// Whether it was "any persona". Never read from `target`.
    pub any: bool,
    /// The name that changed hands: the persona it is given back to, with one press for
    /// everything set aside for that name.
    pub was: String,
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
    /// Whether, and why, **what this machine accepted of the project's grants is not settled
    /// against its history now** (#1543, [`purlis_core::dispatcharrival::unsettled`]). Null
    /// where the last settling answered, and where nothing of the project's is accepted here.
    /// The table says which, at the top and beside each accepted grant of the project's.
    pub project_unsettled: Option<ProjectUnsettled>,
    /// The project's personas now, sorted: the table has a row for each, and anything that
    /// names another is drawn as naming no persona (#1504). Null where they could not be
    /// listed, and then no name is called unknown.
    pub personas: Option<Vec<String>>,
    /// The pairs kept blocked for one chat's life, read-only.
    pub kept_blocked: Vec<DispatchKeptBlocked>,
    /// Your grants set aside because a name in them came to be another persona's.
    pub dormant: Vec<DispatchDormant>,
    /// Names that were seen gone and are a persona again with another definition: every grant
    /// that names one is in force for no chat until the person gives it back (#1504).
    pub returned: Vec<String>,
    /// Names the person can give something back to: each of `returned`, and each persona that
    /// has grants or acceptances set aside for it.
    pub back: Vec<String>,
    /// The project's workspaces now, sorted: what a grant's workspace can be set to (#1505).
    /// Empty where they could not be listed.
    pub workspaces: Vec<String>,
    /// What each persona's definition says it wants to dispatch to (#1502), for the personas
    /// that say anything the question would offer: the table shows it under the persona's
    /// name. **It grants nothing, and nothing here is in force by it.**
    pub wants: Vec<DispatchWants>,
}

/// Why what this machine accepted of the project's grants is not settled now (#1543).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectUnsettled {
    /// No settling has landed since purlis started. A dispatch settles before it is decided,
    /// so where the history reads, an accepted grant counts for it.
    NotYet,
    /// The project's history could not be read just now: no accepted grant of the project's
    /// counts until it can. Each dispatch asks again first.
    Unread,
}

impl From<purlis_core::dispatcharrival::Unsettled> for ProjectUnsettled {
    fn from(why: purlis_core::dispatcharrival::Unsettled) -> Self {
        match why {
            purlis_core::dispatcharrival::Unsettled::NotYet => Self::NotYet,
            purlis_core::dispatcharrival::Unsettled::Unread => Self::Unread,
        }
    }
}

/// What one persona's definition says it wants to dispatch to (#1502): the names its line
/// offers, as [`purlis_core::dispatchwants::of`] reads it, in the order written.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchWants {
    pub persona: String,
    pub wants: Vec<String>,
}

/// What stands in the project at `root`.
fn standing_of(root: &Path) -> DispatchStanding {
    let unaccepted = dispatchgrant::any_unaccepted(root);
    let declined = dispatchgrant::declined(root);
    let mut any: Vec<DispatchAny> = dispatchgrant::any_yours(root)
        .into_iter()
        .map(|asking| DispatchAny {
            asking,
            level: GrantLevel::You,
            waiting: false,
            declined: false,
            workspace: None,
            nowhere: None,
            id: None,
        })
        .collect();
    any.extend(
        dispatchgrant::any_committed_at(root)
            .into_iter()
            .map(|asking| DispatchAny {
                waiting: unaccepted.contains(&asking),
                declined: dispatchgrant::project_grant_said(&asking, dispatchgrant::ANY)
                    .is_some_and(|said| declined.contains(&said)),
                asking,
                level: GrantLevel::Project,
                workspace: None,
                nowhere: None,
                id: None,
            }),
    );
    // "Any persona" limited to one workspace (#1505): mine, then the project's.
    let seen = dispatchwithin::noticed(root);
    let unaccepted_in = dispatchwithin::unaccepted(root);
    let accepted_in = dispatchwithin::accepted(root);
    for (one, at) in dispatchwithin::yours(root) {
        if one.any() {
            any.push(DispatchAny {
                id: Some(limited_id(Level::You, &one)),
                nowhere: seen.why_not(&one.workspace, at),
                asking: one.asking,
                level: GrantLevel::You,
                waiting: false,
                declined: false,
                workspace: Some(one.workspace),
            });
        }
    }
    for one in dispatchwithin::committed_at(root) {
        if one.any() {
            let at = accepted_in
                .iter()
                .find(|(held, _)| *held == one)
                .map(|(_, at)| *at);
            any.push(DispatchAny {
                id: Some(limited_id(Level::Project, &one)),
                waiting: unaccepted_in.contains(&one),
                nowhere: seen.why_not(
                    &one.workspace,
                    at.unwrap_or_else(|| seen.gone(&one.workspace)),
                ),
                asking: one.asking,
                level: GrantLevel::Project,
                declined: false,
                workspace: Some(one.workspace),
            });
        }
    }
    let mut workspaces = purlis_core::workspaces::Plane::open(root.to_path_buf())
        .workspaces()
        .unwrap_or_default();
    workspaces.retain(|name| seen.is_there(name));
    workspaces.sort();
    let state = purlis_core::dispatchdormant::state(root, &[]).unwrap_or_default();
    let personas = purlis_core::dispatchdormant::personas_of(root);
    // Read from each definition as it is now, by the reader the question's boxes come from.
    let wants = personas
        .iter()
        .flatten()
        .map(|persona| DispatchWants {
            wants: dispatchwants::of(root, persona).personas,
            persona: persona.clone(),
        })
        .filter(|one| !one.wants.is_empty())
        .collect();
    DispatchStanding {
        nevers: dispatchgrant::nevers_said(root)
            .into_iter()
            .map(|one| DispatchNever {
                asking: one.asking,
                target: one.target,
                at: one.said.at.and_then(|at| u32::try_from(at).ok()),
                chat: one.said.chat,
            })
            .collect(),
        any,
        nevers_unread: dispatchgrant::nevers_unread(root),
        // The verdict a dispatch is read by, with no git run here.
        project_unsettled: purlis_core::dispatcharrival::unsettled(root).map(Into::into),
        dormant: purlis_core::dispatchdormant::list(root)
            .into_iter()
            .map(|one| DispatchDormant {
                asking: one.asking,
                target: one.target,
                any: one.any,
                was: one.was,
            })
            .collect(),
        personas,
        kept_blocked: Vec::new(),
        returned: state.returned,
        back: state.back,
        workspaces,
        wants,
    }
}

/// The id of a grant limited to one workspace, as [`revoke_dispatch_grant`] takes it.
fn limited_id(level: Level, one: &dispatchwithin::Limited) -> String {
    format!(
        "in{SEP}{}{SEP}{}{SEP}{}{SEP}{}",
        level.word(),
        one.asking,
        one.target,
        one.workspace
    )
}

/// **What stands, as Settings reads it** (#1504): [`standing_of`], with the pairs chats are
/// kept blocked for. **Reading moves nothing and drops nothing.** The one thing it writes is
/// the absence mark of a name a grant holds that is no persona now
/// ([`purlis_core::dispatchdormant::noticed`]): no grant leaves its record, nothing this
/// machine accepted is forgotten, and no grant made for one chat ends.
fn standing_read(root: &Path, store: &Store) -> DispatchStanding {
    if let Err(why) = purlis_core::dispatchdormant::noticed(root, &store.chat_names()) {
        tracing::warn!("purlis: a persona that is gone was not marked so ({why})");
    }
    let mut standing = standing_of(root);
    standing.kept_blocked = store.kept_blocked();
    standing
}

/// **Not on my machine**, for the project's grant of `asking` to `target` (a persona's name,
/// or `*` for any persona): checked to be in the project's file, audited, then this machine's
/// acceptance of it is taken away. The committed file is not changed.
fn decline(root: &Path, asking: &str, target: &str, audit: Audit<'_>) -> Result<(), String> {
    if !dispatchgrant::project_grants(root, asking, target) {
        return Err("purlis changed nothing: the project no longer has that grant.".to_owned());
    }
    audit(
        None,
        &dispatchgrant::Audited {
            act: dispatchgrant::Act::Decline,
            asking: Some(asking),
            target,
            level: Level::Project,
            workspace: None,
        },
    )?;
    dispatchgrant::decline(root, asking, target).inspect_err(|_| {
        // Recorded as followed again, so the log never ends on a grant this machine stopped
        // following when it still follows it.
        if let Err(unsaid) = audit(
            None,
            &dispatchgrant::Audited {
                act: dispatchgrant::Act::Grant,
                asking: Some(asking),
                target,
                level: Level::Project,
                workspace: None,
            },
        ) {
            tracing::warn!(
                "purlis: a decline that was not kept is still recorded as made ({unsaid})"
            );
        }
    })
}

/// **Accept**, for the project's grant of `asking` to `target` (a persona's name, or `*`):
/// Settings' yes to a teammate's grant, or to one declined here. Both names must be personas
/// of the project now, so nothing is accepted for a persona that is not there yet. A pair goes
/// through what the project's Notice goes through ([`Store::acknowledge`]); any persona is
/// acknowledged on this machine and no more. Each is audited before it is in force, and
/// neither can write the committed file.
fn accept(
    store: &Store,
    ground: &Ground<'_>,
    known: &dyn Fn(&str) -> bool,
    asking: &str,
    target: &str,
) -> Result<(), String> {
    let any = target == dispatchgrant::ANY;
    let mut names = vec![asking];
    if !any {
        names.push(target);
    }
    for name in names {
        if !known(name) {
            return Err(format!(
                "This project has no persona named {}, so nothing was accepted.",
                purlis_core::shown::short(name)
            ));
        }
    }
    if !dispatchgrant::project_grants(ground.root, asking, target) {
        return Err("purlis changed nothing: the project no longer has that grant.".to_owned());
    }
    if any {
        // **Accepting is this machine's act**: it records the acceptance and nothing else.
        // It never goes through the writer of the committed file, so a grant the file lost a
        // moment ago is not written back into it by a press that says "on this machine".
        let audited = dispatchgrant::Audited {
            act: dispatchgrant::Act::Grant,
            asking: Some(asking),
            target: dispatchgrant::ANY,
            level: Level::Project,
            workspace: None,
        };
        (ground.audit)(None, &audited)?;
        dispatchgrant::accept_any_of_the_project(ground.root, asking).inspect_err(|_| {
            if let Err(unsaid) = (ground.audit)(
                None,
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::Revoke,
                    ..audited
                },
            ) {
                tracing::warn!(
                    "purlis: an acceptance that was not kept is still recorded as made ({unsaid})"
                );
            }
        })?;
        store.start_what_is_covered(ground);
        return Ok(());
    }
    let pair = Pair::new(asking, target)?;
    store.acknowledge(ground, &[pair.to_string()])
}

/// **Give back to `name`**: the person's one acknowledgement that the persona of that name
/// today may have what an earlier persona of the name had. Checked first, then the
/// acknowledgement itself is recorded ([`dispatchgrant::Act::GiveBack`]), so nothing is given
/// back that the log would not take. Then the grants set aside for the name are in force
/// again, what this machine had accepted of the project's grants for it is accepted again,
/// and its absence mark is lifted. Each grant that came back is recorded after it as a grant,
/// for me or of the project's, as it is kept: which ones come back is only known once the one
/// write has chosen them.
fn give_back(root: &Path, name: &str, audit: Audit<'_>) -> Result<(), String> {
    purlis_core::dispatchdormant::can_give_back(root, name)?;
    audit(
        None,
        &dispatchgrant::Audited {
            act: dispatchgrant::Act::GiveBack,
            asking: Some(name),
            target: name,
            level: Level::You,
            workspace: None,
        },
    )?;
    let back = purlis_core::dispatchdormant::give_back(root, name)?;
    let grants = back
        .grants
        .iter()
        .map(|one| (one.asking.as_str(), one.target.as_str(), Level::You));
    let accepted = back.accepted.iter().filter_map(|one| {
        let (asking, target) = one.said.split_once(" -> ")?;
        Some((asking, target, Level::Project))
    });
    for (asking, target, level) in grants.chain(accepted) {
        audit(
            None,
            &dispatchgrant::Audited {
                act: dispatchgrant::Act::Grant,
                asking: Some(asking),
                target,
                level,
                workspace: None,
            },
        )
        .map_err(|unsaid| {
            format!(
                "The grants are back in force, and purlis's event log did not take the \
                 record of {asking} to {target} ({unsaid})."
            )
        })?;
    }
    Ok(())
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
            workspace: None,
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
        workspace: None,
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
            workspace: None,
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
    let held = planes.held(&plane)?;
    Ok(standing_read(held.root(), held.dispatch_grants()))
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
    Ok(standing_read(root, held.dispatch_grants()))
}

/// **Any persona**, from Settings: chats running as `asking` may dispatch to every persona of
/// the project, one added later included, for you on this machine or for everyone in the
/// project. Audited, then kept; a dispatch waiting on the person that it covers starts.
/// Answers what stands now.
// On a blocking thread ([`SETTLES`]): for everyone in the project it ends in this machine's
// acceptance, which settles against git's history.
#[tauri::command]
#[specta::specta]
pub async fn allow_dispatch_to_any(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    level: GrantLevel,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("allowing any persona", move || {
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
        arrival_moved(&plane);
        Ok(standing_read(root, held.dispatch_grants()))
    })
    .await
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
    held.dispatch_grants().drop_once(Some(&asking), None);
    arrival_moved(&plane);
    Ok(standing_read(root, held.dispatch_grants()))
}

/// **Not on my machine** on Settings' table (#1504): this machine stops following one grant of
/// the project's, a pair or any persona (`target` is `*`). The committed file is not changed,
/// so teammates keep it. Audited first. Answers what stands now.
#[tauri::command]
#[specta::specta]
pub fn decline_project_dispatch(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    decline(root, &asking, &target, &|number, audited| {
        held.hooks().record_dispatch_grant(root, number, audited)
    })?;
    held.dispatch_grants().drop_once(
        Some(&asking),
        (target != dispatchgrant::ANY).then_some(target.as_str()),
    );
    arrival_moved(&plane);
    Ok(standing_read(root, held.dispatch_grants()))
}

/// **Accept** on Settings' table (#1504): this machine follows one grant of the project's from
/// now on, a pair or any persona (`target` is `*`), a teammate's that was waiting or one
/// declined here. Audited first; a dispatch waiting on the person that it covers starts.
/// Answers what stands now.
// On a blocking thread ([`SETTLES`]): accepting settles against git's history.
#[tauri::command]
#[specta::specta]
pub async fn accept_project_dispatch(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("accepting the project's dispatch grant", move || {
        let root = held.root();
        let personas = purlis_core::dispatchdormant::personas_of(root).unwrap_or_default();
        with_ground(&held, |ground| {
            accept(
                held.dispatch_grants(),
                ground,
                &|name| personas.iter().any(|one| one == name),
                &asking,
                &target,
            )
        })?;
        arrival_moved(&plane);
        Ok(standing_read(root, held.dispatch_grants()))
    })
    .await
}

/// **Give back** on Settings' table (#1504): the person's one acknowledgement for the persona
/// `name`, which has the name of a persona that was seen gone. What was set aside for it is
/// in force again, and what was held back while it waited counts again. Refused while the
/// name is no persona. Recorded first. Answers what stands now.
// On a blocking thread ([`SETTLES`]): what was accepted for the name is settled against the
// project's history before it is given back.
#[tauri::command]
#[specta::specta]
pub async fn give_back_dispatch(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("giving the grants back", move || {
        let root = held.root();
        with_ground(&held, |ground| {
            give_back(root, &name, ground.audit)?;
            held.dispatch_grants().start_what_is_covered(ground);
            Ok::<(), String>(())
        })?;
        Ok(standing_read(root, held.dispatch_grants()))
    })
    .await
}

/// **Remove** on a grant Settings shows set aside (#1504): that one entry is taken out for
/// good, the one set aside as "any persona" where `any`, else the pair. It was in force for
/// no chat, so nothing changes for any. Answers what stands now.
#[tauri::command]
#[specta::specta]
pub fn remove_dormant_dispatch(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
    any: bool,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    if !purlis_core::dispatchdormant::remove(root, &asking, &target, any)? {
        return Err("purlis removed nothing: that grant is no longer there.".to_owned());
    }
    Ok(standing_read(root, held.dispatch_grants()))
}

// ---- a grant limited to one workspace (#1505) ------------------------------------------------

/// **Allow, in any workspace**, on a dispatch's Notice, at `level`: the explicit wider choice
/// (#1505). [`allow_dispatch`] is the narrower one, which holds in the workspace the task
/// works in. The pair and the workspace are the app's record of the held dispatch `id`, never
/// the window's word. Audited, then kept, and the dispatch starts.
///
/// `also` and `shown` are [`allow_dispatch`]'s (#1502): each ticked persona the question still
/// offers is kept at the same level **and in any workspace too**, as the asked pair is.
// On a blocking thread ([`SETTLES`]), as [`allow_dispatch`] is and for its reason.
#[tauri::command]
#[specta::specta]
pub async fn allow_dispatch_anywhere(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: u32,
    level: GrantLevel,
    also: Vec<String>,
    shown: String,
) -> Result<DispatchAllowed, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("allowing the dispatch", move || {
        let allowed = with_ground(&held, |ground| {
            held.dispatch_grants().allow_anywhere_with(
                ground,
                id,
                level.into(),
                &Ticked {
                    also: &also,
                    shown: Some(&shown),
                },
            )
        })
        .map(|said| DispatchAllowed { said });
        // An Allow for everyone answers a grant of the project's that was waiting (#1506),
        // the wider Allow as the narrower one does.
        arrival_moved(&plane);
        allowed
    })
    .await
}

/// **Sets where the grant of `asking` to `target` (`*`: any persona) holds**, at `level`
/// (`You` or `Project`), from `from` to `to` (`None`: any workspace), in the project at
/// `ground.root` where `known` says which names are its personas.
///
/// Checked first: the grant is there as `from` says, the names are personas, and `to` is a
/// workspace of the project now. Then audited as what it is, the grant as it was taken back
/// and the grant as it now is made, each with its workspace, before the one write. A write
/// that fails is recorded the other way round, so the log never ends on a grant that is not
/// the one kept. A project grant's change edits the committed file.
fn set_workspace(
    store: &Store,
    ground: &Ground<'_>,
    known: &dyn Fn(&str) -> bool,
    asking: &str,
    target: &str,
    level: Level,
    (from, to): (&dispatchwithin::Within, &dispatchwithin::Within),
) -> Result<(), String> {
    use dispatchwithin::Within;
    let root = ground.root;
    let any = target == dispatchgrant::ANY;
    let gone = || "purlis changed nothing: that grant is no longer there.".to_owned();
    let there = match level {
        Level::Chat => {
            return Err(
                "A grant made for one chat holds for the task it was allowed for, and is not \
                 changed. Revoke it, and the chat asks again."
                    .to_owned(),
            );
        }
        Level::You => dispatchwithin::yours_holds(root, asking, target, from),
        Level::Project => dispatchwithin::project_holds(root, asking, target, from),
    };
    if !there {
        return Err(gone());
    }
    if from == to && *to == Within::Any {
        return Err("That grant holds in any workspace already.".to_owned());
    }
    // **Count it again, for a project grant, is this machine's act** (the same workspace,
    // set again): it accepts the grant as the file holds it, for the workspace of that name
    // that is there now, and never rewrites the committed file.
    if let (Level::Project, Within::Workspace(workspace), true) = (level, to, from == to) {
        let one = dispatchwithin::Limited::new(asking, target, workspace)?;
        return accept_in(store, ground, known, &one);
    }
    let mut names = vec![asking];
    if !any {
        names.push(target);
    }
    for name in names {
        if !known(name) {
            return Err(format!(
                "This project has no persona named {}, so nothing was changed.",
                purlis_core::shown::short(name)
            ));
        }
    }
    dispatchwithin::can_set(root, asking, target, to)?;
    if level == Level::Project {
        purlis_core::settings::dispatch::can_set_within(root, asking, target, from, to)?;
    }
    let was = dispatchgrant::Audited {
        act: dispatchgrant::Act::Revoke,
        asking: Some(asking),
        target,
        level,
        workspace: from.workspace(),
    };
    let now = dispatchgrant::Audited {
        act: dispatchgrant::Act::Grant,
        workspace: to.workspace(),
        ..was
    };
    (ground.audit)(None, &was)?;
    (ground.audit)(None, &now)?;
    let written = match level {
        Level::Project => {
            purlis_core::settings::dispatch::write_within(root, asking, target, from, to)
        }
        _ => match dispatchwithin::set_yours(root, asking, target, from, to) {
            Ok(true) => Ok(()),
            Ok(false) => Err(gone()),
            Err(why) => Err(format!("purlis could not keep the change: {why}")),
        },
    };
    if let Err(why) = written {
        // Recorded as put back, so the log never ends on a grant that is not the one kept.
        // **Only here, where nothing was written.**
        for back in [
            dispatchgrant::Audited {
                act: dispatchgrant::Act::Revoke,
                ..now
            },
            dispatchgrant::Audited {
                act: dispatchgrant::Act::Grant,
                ..was
            },
        ] {
            if let Err(unsaid) = (ground.audit)(None, &back) {
                tracing::warn!(
                    "purlis: a dispatch grant's workspace that was not changed is still \
                     recorded as changed ({unsaid})"
                );
            }
        }
        return Err(why);
    }
    // Whatever one start was still kept for the pair was allowed under the grant as it was.
    store.drop_once(Some(asking), (!any).then_some(target));
    // **The project's file holds the change from here on.** Where this machine cannot record
    // that it follows it, that is said and nothing is recorded as put back: the log's last
    // word is the grant the file holds, which is the grant the team has.
    if level == Level::Project {
        purlis_core::settings::dispatch::follow_within(root, asking, target, from, to)?;
    }
    // What the list says of when it was made, and from which chat, goes with the grant.
    let said = |within: &Within| match within {
        Within::Any => format!("{asking} -> {target}"),
        Within::Workspace(workspace) => format!("{asking} -> {target} in {workspace}"),
    };
    if let Some(record) = sandbox::local::made(root)
        .into_iter()
        .find(|one| one.what == WHAT && one.target == said(from) && one.level == level.word())
    {
        let _ = sandbox::local::forget_made(root, WHAT, &said(from), level.word());
        let _ = sandbox::local::record_made(
            root,
            sandbox::local::Made {
                target: said(to),
                ..record
            },
        );
    }
    store.start_what_is_covered(ground);
    Ok(())
}

/// **Accept**, for the project's grant of `asking` to `target` (`*`: any persona) limited to
/// `workspace`: this machine's yes, in Settings. Both names must be personas of the project
/// now and the file must hold the grant. Audited before it is in force; the committed file is
/// never written.
pub(crate) fn accept_in(
    store: &Store,
    ground: &Ground<'_>,
    known: &dyn Fn(&str) -> bool,
    one: &dispatchwithin::Limited,
) -> Result<(), String> {
    // Nothing is accepted, or recorded, for a name that is no workspace now: the acceptance
    // would belong to whatever was made under that name next.
    if !dispatchwithin::Seen::read(ground.root).is_there(&one.workspace) {
        return Err(dispatchwithin::not_there_said(&one.workspace));
    }
    let mut names = vec![one.asking.as_str()];
    if !one.any() {
        names.push(&one.target);
    }
    for name in names {
        if !known(name) {
            return Err(format!(
                "This project has no persona named {}, so nothing was accepted.",
                purlis_core::shown::short(name)
            ));
        }
    }
    if !dispatchwithin::committed_at(ground.root).contains(one) {
        return Err("purlis changed nothing: the project no longer has that grant.".to_owned());
    }
    let audited = dispatchgrant::Audited {
        act: dispatchgrant::Act::Grant,
        asking: Some(&one.asking),
        target: &one.target,
        level: Level::Project,
        workspace: Some(&one.workspace),
    };
    (ground.audit)(None, &audited)?;
    dispatchwithin::accept(ground.root, one).inspect_err(|_| {
        if let Err(unsaid) = (ground.audit)(
            None,
            &dispatchgrant::Audited {
                act: dispatchgrant::Act::Revoke,
                ..audited
            },
        ) {
            tracing::warn!(
                "purlis: an acceptance that was not kept is still recorded as made ({unsaid})"
            );
        }
    })?;
    store.start_what_is_covered(ground);
    Ok(())
}

/// **Not on my machine**, for the project's limited grant `one`: audited, then this machine's
/// acceptance of it is taken away. The committed file is not changed, and the grant waits in
/// Settings for Accept.
///
/// **Only for one this machine accepted** (#1543): a limited grant nobody here accepted
/// already allows nothing here, and has no decline of its own to keep (the project's Notice
/// never tells of it), so nothing is audited for it and nothing changes.
fn decline_in(root: &Path, one: &dispatchwithin::Limited, audit: Audit<'_>) -> Result<(), String> {
    if !dispatchwithin::committed_at(root).contains(one) {
        return Err("purlis changed nothing: the project no longer has that grant.".to_owned());
    }
    if !dispatchwithin::accepted(root)
        .iter()
        .any(|(accepted, _)| accepted == one)
    {
        return Err(NOT_ACCEPTED_IN.to_owned());
    }
    audit(
        None,
        &dispatchgrant::Audited {
            act: dispatchgrant::Act::Decline,
            asking: Some(&one.asking),
            target: &one.target,
            level: Level::Project,
            workspace: Some(&one.workspace),
        },
    )?;
    dispatchwithin::unaccept(root, one)
        .map(|_| ())
        .map_err(|why| format!("purlis could not record it: {why}"))
}

/// What Not on my machine is refused with for a limited grant this machine never accepted.
const NOT_ACCEPTED_IN: &str = "purlis changed nothing: this machine never accepted that \
     grant, so it already allows nothing here.";

/// **Changes which workspace a grant holds in**, on Settings' table (#1505): the grant of
/// `asking` to `target` (`*`: any persona) at `level` (`you` or `project`), which holds in
/// `from` now (null: any workspace), is set to hold in `to` (null: any workspace). Narrowing
/// and widening alike are the person's confirmed press; a project grant's change edits the
/// committed file. Setting a grant to the workspace it names already confirms it for the
/// workspace of that name that is there now. Audited first. Answers what stands now.
// On a blocking thread ([`SETTLES`]): widening a project grant to any workspace ends in this
// machine's acceptance of the pair, which settles against git's history.
#[tauri::command]
#[specta::specta]
pub async fn set_dispatch_workspace(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
    level: GrantLevel,
    from: Option<String>,
    to: Option<String>,
) -> Result<DispatchStanding, String> {
    use dispatchwithin::Within;
    let held = planes.held(&plane)?;
    crate::off_the_window("changing where the dispatch grant holds", move || {
        let root = held.root();
        let personas = purlis_core::dispatchdormant::personas_of(root).unwrap_or_default();
        with_ground(&held, |ground| {
            set_workspace(
                held.dispatch_grants(),
                ground,
                &|name| personas.iter().any(|one| one == name),
                &asking,
                &target,
                level.into(),
                (&Within::of(from.as_deref()), &Within::of(to.as_deref())),
            )
        })?;
        // A project grant widened to any workspace answers one that was waiting.
        arrival_moved(&plane);
        Ok(standing_read(root, held.dispatch_grants()))
    })
    .await
}

/// **Accept** on Settings' table, for a grant of the project's limited to one workspace
/// (#1505): this machine follows it from now on, for work in `workspace` only. Audited first.
/// Answers what stands now.
// On a blocking thread ([`SETTLES`]): an acceptance is bound to the project's history.
#[tauri::command]
#[specta::specta]
pub async fn accept_project_dispatch_in(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
    workspace: String,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("accepting the project's dispatch grant", move || {
        let root = held.root();
        let one = dispatchwithin::Limited::new(&asking, &target, &workspace)?;
        let personas = purlis_core::dispatchdormant::personas_of(root).unwrap_or_default();
        with_ground(&held, |ground| {
            accept_in(
                held.dispatch_grants(),
                ground,
                &|name| personas.iter().any(|one| one == name),
                &one,
            )
        })?;
        Ok(standing_read(root, held.dispatch_grants()))
    })
    .await
}

/// **Not on my machine** on Settings' table, for a grant of the project's limited to one
/// workspace (#1505). The committed file is not changed. Audited first. Answers what stands
/// now.
#[tauri::command]
#[specta::specta]
pub fn decline_project_dispatch_in(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    asking: String,
    target: String,
    workspace: String,
) -> Result<DispatchStanding, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    let one = dispatchwithin::Limited::new(&asking, &target, &workspace)?;
    decline_in(root, &one, &|number, audited| {
        held.hooks().record_dispatch_grant(root, number, audited)
    })?;
    held.dispatch_grants().drop_once(
        Some(&one.asking),
        (!one.any()).then_some(one.target.as_str()),
    );
    Ok(standing_read(root, held.dispatch_grants()))
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
    /// Whether it is the project's and the person said "Not on my machine" to it (#1504): it
    /// is `waiting` too, and is not told as new until it is accepted in Settings.
    pub declined: bool,
    /// The workspace it is limited to (#1505); null where it holds in any workspace. For a
    /// grant made for one chat: the workspace the task it was allowed for works in, null for
    /// the project's root, and it covers that chat's dispatches there only.
    pub workspace: Option<String>,
    /// Why it covers nothing because of its workspace, where it does not: the workspace is
    /// gone, or one was made again under its name since. Null where its workspace stands.
    pub nowhere: Option<String>,
}

/// A pair policy locks, as Settings shows it locked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchLock {
    pub asking: String,
    pub target: String,
}

/// Everything Settings shows of dispatch grants: the grants, and what policy locks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchGrants {
    pub grants: Vec<DispatchGrant>,
    /// Where policy forbids every dispatch: its sentence, naming who set it.
    pub all_locked: Option<String>,
    /// The pairs policy locks.
    pub locked_pairs: Vec<DispatchLock>,
    /// "Locked by policy, set by <who> in <file>.", where a policy locks anything of dispatch.
    pub locked_by: Option<String>,
}

/// Whether `line` of the project's file is the line granting `pair`: the asking persona's key,
/// quoted or not, set to a list that names the target.
fn grants_line(line: &str, pair: &Pair) -> bool {
    let rest = line.trim_start();
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let Some(rest) = rest.strip_prefix(pair.asking.as_str()) else {
        return false;
    };
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    rest.trim_start()
        .strip_prefix('=')
        .is_some_and(|list| list.contains(&format!("\"{}\"", pair.target)))
}

/// Who last committed the line granting each of `pairs` in the project's committed file, and
/// when: the project's history, asked of git **once for them all** (#1464,
/// [`purlis_core::committedby`]). `None` for one git has nothing to say of (not committed yet).
fn committed_by(root: &Path, pairs: &[Pair]) -> Vec<Option<purlis_core::committedby::Committed>> {
    let wanted: Vec<_> = pairs
        .iter()
        .map(|pair| move |line: &str| grants_line(line, pair))
        .collect();
    purlis_core::committedby::last_touching(root, &wanted)
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
    let declined = dispatchgrant::declined(root);
    let mut out: Vec<DispatchGrant> = store
        .chat_grants(open)
        .into_iter()
        .map(|(id, one)| {
            let asking = one.pair.asking.clone();
            DispatchGrant {
                id: format!(
                    "chat{SEP}{id}{SEP}{}{SEP}{}{SEP}{}",
                    asking.as_deref().unwrap_or_default(),
                    one.pair.target,
                    one.works_in.as_deref().unwrap_or_default()
                ),
                workspace: one.works_in,
                nowhere: None,
                locked: locked(asking.as_deref(), &one.pair.target),
                asking,
                target: one.pair.target,
                level: GrantLevel::Chat,
                by: None,
                at: u32::try_from(one.at).ok(),
                chat: Some(one.chat),
                waiting: false,
                declined: false,
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
            declined: false,
            workspace: None,
            nowhere: None,
        }
    };
    out.extend(
        dispatchgrant::yours(root)
            .iter()
            .map(|pair| row(pair, Level::You)),
    );
    let committed = dispatchgrant::committed_at(root);
    for (pair, by) in committed.iter().zip(committed_by(root, &committed)) {
        let mut one = row(pair, Level::Project);
        one.waiting = unseen.contains(pair);
        one.declined = one.waiting && declined.contains(&pair.to_string());
        if let Some(by) = by {
            one.at = u32::try_from(by.at).ok().or(one.at);
            one.by = Some(by.by);
        }
        out.push(one);
    }
    // The grants limited to one workspace (#1505), pairs only: "any persona" is listed with
    // what stands. Mine, then the project's.
    let seen = dispatchwithin::noticed(root);
    let limited = |level: Level, one: &dispatchwithin::Limited, at: u32| {
        let record = made
            .iter()
            .find(|made| {
                made.what == WHAT && made.target == one.to_string() && made.level == level.word()
            })
            .cloned();
        DispatchGrant {
            id: limited_id(level, one),
            asking: Some(one.asking.clone()),
            target: one.target.clone(),
            level: level.into(),
            by: None,
            at: record.as_ref().and_then(|one| u32::try_from(one.at).ok()),
            chat: record.and_then(|one| one.chat),
            locked: locked(Some(&one.asking), &one.target),
            waiting: false,
            declined: false,
            workspace: Some(one.workspace.clone()),
            nowhere: seen.why_not(&one.workspace, at),
        }
    };
    for (one, at) in dispatchwithin::yours(root) {
        if !one.any() {
            out.push(limited(Level::You, &one, at));
        }
    }
    let accepted = dispatchwithin::accepted(root);
    for one in dispatchwithin::committed_at(root) {
        if one.any() {
            continue;
        }
        let at = accepted
            .iter()
            .find(|(held, _)| *held == one)
            .map(|(_, at)| *at);
        let mut row = limited(
            Level::Project,
            &one,
            at.unwrap_or_else(|| seen.gone(&one.workspace)),
        );
        row.waiting = at.is_none();
        out.push(row);
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
        ["chat", chat, asking, target, workspace] => {
            let pair = ChatPair {
                asking: (!asking.is_empty()).then(|| (*asking).to_owned()),
                target: (*target).to_owned(),
            };
            let works_in = (!workspace.is_empty()).then_some(*workspace);
            if !store.chat_pairs(Some(chat), works_in).contains(&pair) {
                return Err(gone());
            }
            audit(
                None,
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::Revoke,
                    asking: pair.asking.as_deref(),
                    target,
                    level: Level::Chat,
                    workspace: works_in,
                },
            )?;
            store.revoke_chat(chat, &pair, works_in);
            store.drop_once(pair.asking.as_deref(), Some(target));
            Ok(())
        }
        // A grant limited to one workspace (#1505), a pair or any persona (`*`).
        ["in", level, asking, target, workspace] => {
            let level = Level::of_word(level)
                .filter(|level| *level != Level::Chat)
                .ok_or_else(gone)?;
            let one =
                dispatchwithin::Limited::new(asking, target, workspace).map_err(|_| gone())?;
            let there = if level == Level::Project {
                dispatchwithin::committed_at(root).contains(&one)
            } else {
                dispatchwithin::yours(root)
                    .iter()
                    .any(|(held, _)| *held == one)
            };
            if !there {
                return Err(gone());
            }
            audit(
                None,
                &dispatchgrant::Audited {
                    act: dispatchgrant::Act::Revoke,
                    asking: Some(&one.asking),
                    target: &one.target,
                    level,
                    workspace: Some(&one.workspace),
                },
            )?;
            if level == Level::Project {
                purlis_core::settings::dispatch::revoke_in(root, &one)?;
            } else {
                dispatchwithin::revoke_yours(root, &one)
                    .map_err(|why| format!("purlis could not revoke it: {why}"))?;
            }
            store.drop_once(
                Some(&one.asking),
                (!one.any()).then_some(one.target.as_str()),
            );
            if let Err(why) =
                sandbox::local::forget_made(root, WHAT, &one.to_string(), level.word())
            {
                tracing::warn!("purlis: a revoked dispatch grant's record was left behind ({why})");
            }
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
                    workspace: None,
                },
            )?;
            if level == Level::Project {
                purlis_core::settings::dispatch::revoke(root, &pair)?;
            } else {
                sandbox::local::revoke_dispatch(root, &pair.asking, &pair.target)
                    .map_err(|why| format!("purlis could not revoke it: {why}"))?;
            }
            store.drop_once(Some(&pair.asking), Some(&pair.target));
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
    }
}

/// The ids of the chats `held` has open.
fn open_ids(held: &crate::planes::Held) -> std::collections::HashSet<String> {
    held.chats()
        .open_now()
        .into_iter()
        .filter_map(|open| held.chats().recorded_chat(open.session)?.identity.id)
        .collect()
}

/// Every dispatch grant in force here, and what policy locks: for Settings' list.
// On a blocking thread ([`READS_HISTORY`]): who committed each of the project's grants is
// asked of git, once for the page.
#[tauri::command]
#[specta::specta]
pub async fn dispatch_grants(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<DispatchGrants, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading the dispatch grants", move || {
        let open = open_ids(&held);
        Ok(state_of(held.root(), held.dispatch_grants(), &|id| {
            open.contains(id)
        }))
    })
    .await
}

/// **Revoke** on Settings' list of dispatch grants: the grant called `id` is audited and taken
/// out, so the next dispatch across its pair asks again. Answers the list as it is now.
// On a blocking thread ([`READS_HISTORY`]): the list it answers asks git who committed each of
// the project's grants.
#[tauri::command]
#[specta::specta]
pub async fn revoke_dispatch_grant(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: String,
) -> Result<DispatchGrants, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("revoking the dispatch grant", move || {
        let root = held.root();
        revoke(root, held.dispatch_grants(), &id, &|number, audited| {
            held.hooks().record_dispatch_grant(root, number, audited)
        })?;
        arrival_moved(&plane);
        let open = open_ids(&held);
        Ok(state_of(root, held.dispatch_grants(), &|id| {
            open.contains(id)
        }))
    })
    .await
}

// ---- a teammate's grant, when it arrives (#1506) ---------------------------------------------

/// The event the window is sent when what waits of the project's grants may have moved: an
/// answer was given, here or in Settings, or a settling after the watcher's event finished.
/// Its payload is a [`DispatchArrivalMoved`].
pub const ARRIVAL: &str = "dispatch-arrival";

/// What [`ARRIVAL`] carries.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchArrivalMoved {
    pub plane: PlaneId,
}

/// Told each time what waits of a project's grants may have moved, for the window.
pub type ArrivalTeller = Arc<dyn Fn(DispatchArrivalMoved) + Send + Sync + 'static>;

static ARRIVAL_TELL: OnceLock<ArrivalTeller> = OnceLock::new();

/// Sends every such move to `tell` from now on: the window, once, as the app starts.
pub fn telling_arrival(tell: ArrivalTeller) {
    let _ = ARRIVAL_TELL.set(tell);
}

/// Says that what waits of `plane`'s grants may have moved.
fn arrival_moved(plane: &PlaneId) {
    if let Some(tell) = ARRIVAL_TELL.get() {
        tell(DispatchArrivalMoved {
            plane: plane.clone(),
        });
    }
}

/// Whether a change the watcher told of may be one this machine's acceptances are settled
/// for: the project's own file, or a burst the watcher could not place. A pull that moves the
/// checkout and leaves that file as it was is caught where a dispatch is decided and where
/// the window reads what waits, which both settle.
pub fn concerns_grants(what: Option<&[purlis_core::planechange::Change]>) -> bool {
    what.is_none_or(|changes| {
        changes
            .iter()
            .any(|one| one.kind == purlis_core::planechange::Kind::Project)
    })
}

/// **The watcher said the project at `root` moved on disk** (a pull, a branch switched, a
/// hand's edit). **Nothing is settled on the watcher's thread, and the window is told of the
/// change first**: where the change may be the project's file, the acceptances are settled
/// against git's history on a thread of its own ([`purlis_core::dispatcharrival::settle`],
/// one settling per project at a time), and the window is then told to read what waits again.
/// A grant merely absent from the file on disk drops nothing.
pub fn project_moved(
    plane: &PlaneId,
    root: &Path,
    what: Option<&[purlis_core::planechange::Change]>,
) {
    if !concerns_grants(what) {
        return;
    }
    let (plane, root) = (plane.clone(), root.to_path_buf());
    let settled = std::thread::Builder::new()
        .name("dispatch-settle".to_owned())
        .spawn(move || {
            purlis_core::dispatcharrival::settle(&root);
            arrival_moved(&plane);
        });
    if let Err(why) = settled {
        // Not settled now: the next dispatch and the next read of what waits both settle.
        tracing::warn!("purlis: the project's dispatch grants were not settled ({why})");
    }
}

/// Why a grant the person accepted before waits for a yes again.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "why", rename_all = "camelCase")]
pub enum DispatchAgain {
    /// A commit of the project's history took it out, and the file holds it again.
    TakenOut,
    /// purlis could not read the project's history since it was accepted.
    Unread,
    /// The persona `name` was not in the project for a time, and one of that name is.
    Persona { name: String },
}

/// One grant of the project's waiting for the person's answer, as the Notice lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchArrived {
    /// What an answer is sent by: the grant exactly as it is shown here. One that is shown
    /// another way by the time the answer comes is not answered. **Everything the Notice
    /// shows of a grant belongs in it.**
    pub id: String,
    /// The persona whose chats it lets dispatch.
    pub asking: String,
    /// The persona they may dispatch to; `*` where `any`.
    pub target: String,
    /// Whether it is "any persona": every persona of the project, ones added later included.
    /// **The Notice tells of it and may decline it. It is accepted in Settings only.**
    pub any: bool,
    /// A name in it that is no persona of the project as it is checked out here. It can be
    /// used by nothing, and Accept leaves it out.
    pub undefined: Option<String>,
    /// Why it is asked again, where the person accepted it before.
    pub again: Option<DispatchAgain>,
    /// **What the target persona works with**, in the words the dispatch question says it
    /// (#1502, [`purlis_core::dispatchwants::Access::said`]): its vaults, its hosts, and what
    /// it may itself dispatch to. It is what a yes to the pair reaches, so the Notice says it
    /// beside the pair, and it is part of `id`. Null for "any persona", which names no one
    /// persona, and for a grant naming a persona the project does not define.
    pub works_with: Option<String>,
}

/// One grant the person had accepted that the project took away, as the Notice says it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchGone {
    /// What "told" is sent by.
    pub id: String,
    pub asking: String,
    /// The persona's name; `*` where `any`.
    pub target: String,
    pub any: bool,
}

/// What the project's dispatch grants ask of the person now.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchArrival {
    /// The grants waiting for an answer, "any persona" first. None is in force here.
    pub waiting: Vec<DispatchArrived>,
    /// The grants accepted here that a commit took out of the project's file: said once,
    /// asking nothing.
    pub gone: Vec<DispatchGone>,
    /// Whether the project's git history could not be asked just now. While it cannot, no
    /// grant of the project's that was accepted here counts, and nothing can be accepted.
    pub unread: bool,
}

/// What an answer to the arrival Notice did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchArrivalAnswered {
    /// Where the list was no longer as it was shown: the sentence saying so, and what was
    /// answered all the same. Null where everything shown was answered as shown.
    pub said: Option<String>,
    /// What waits now.
    pub arrival: DispatchArrival,
}

/// **What the target of the waiting grant `one` works with**, read from the project at `root`
/// as it is now under `locks` (#1502's one function for those words, [`Access::at`]): what
/// the arrival Notice says beside the pair. None for "any persona" and for a grant naming a
/// persona the project does not define, which nothing can use.
fn arrived_access(
    root: &Path,
    locks: &sandbox::policy::Locks,
    one: &purlis_core::dispatcharrival::Arrived,
) -> Option<Access> {
    (one.target != dispatchgrant::ANY && one.undefined.is_none())
        .then(|| Access::at(root, locks, &one.target))
}

/// `one` exactly as it is shown: the grant, whether it can be used, why it is asked again,
/// **and what its target works with** (`works_with`, as a digest of all of it, what the
/// sentence clips included: [`Offer::stamp`]). So a yes sent after the target gained a vault,
/// a host or a persona it may itself dispatch to is a yes to a grant that is no longer as it
/// was shown, and answers nothing.
fn arrived_id(one: &purlis_core::dispatcharrival::Arrived, works_with: Option<&Access>) -> String {
    use purlis_core::dispatcharrival::Again;
    let again = match &one.again {
        None => String::new(),
        Some(Again::TakenOut) => "taken-out".to_owned(),
        Some(Again::Unread) => "unread".to_owned(),
        Some(Again::Persona(name)) => format!("persona {name}"),
    };
    let access = works_with.map_or_else(String::new, |access| {
        Offer {
            target: access.clone(),
            also: Vec::new(),
        }
        .stamp()
    });
    format!(
        "{}{SEP}{}{SEP}{again}{SEP}{access}",
        one.said(),
        one.undefined.as_deref().unwrap_or_default(),
    )
}

/// The id of the waiting grant `one` as the project at `root` stands now.
fn arrived_id_at(
    root: &Path,
    locks: &sandbox::policy::Locks,
    one: &purlis_core::dispatcharrival::Arrived,
) -> String {
    arrived_id(one, arrived_access(root, locks, one).as_ref())
}

/// `arrival` as the window is told it, in the project at `root`.
fn arrival_told(root: &Path, arrival: purlis_core::dispatcharrival::Arrival) -> DispatchArrival {
    use purlis_core::dispatcharrival::Again;
    let locks = sandbox::policy::Locks::of(root);
    DispatchArrival {
        waiting: arrival
            .waiting
            .into_iter()
            .map(|one| (arrived_access(root, &locks, &one), one))
            .map(|(access, one)| DispatchArrived {
                id: arrived_id(&one, access.as_ref()),
                works_with: access.as_ref().map(Access::said),
                any: one.target == dispatchgrant::ANY,
                asking: one.asking,
                target: one.target,
                undefined: one.undefined,
                again: one.again.map(|why| match why {
                    Again::TakenOut => DispatchAgain::TakenOut,
                    Again::Unread => DispatchAgain::Unread,
                    Again::Persona(name) => DispatchAgain::Persona { name },
                }),
            })
            .collect(),
        gone: arrival
            .gone
            .into_iter()
            .filter_map(|said| {
                let (asking, target) = said.split_once(" -> ")?;
                Some(DispatchGone {
                    asking: asking.to_owned(),
                    target: target.to_owned(),
                    any: target == dispatchgrant::ANY,
                    id: said.clone(),
                })
            })
            .collect(),
        unread: arrival.unread,
    }
}

/// What waits in the project at `root`, as the window is told it. **Settles first, which
/// runs git**: for a blocking thread, never the one that pumps the window.
fn arrival_of(root: &Path) -> DispatchArrival {
    arrival_told(root, purlis_core::dispatcharrival::arrival(root))
}

/// What accepting "any persona" from a Notice is refused with.
pub const ANY_IS_SETTINGS: &str = "Any persona is accepted in Settings \u{203a} Project \u{203a} \
     Dispatch, and never from a Notice. Nothing was accepted.";

/// **Accept, or Not on my machine, for `shown`**: grants the arrival Notice listed, each by
/// the id it was shown under. `listed` is every id the Notice listed, answered or not (Accept
/// answers the named pairs of a list that also tells of "any persona"): what waits now and
/// was not listed is what arrived since.
///
/// - The records are first brought up to what the project's personas are now, as before any
///   judged dispatch, and the list is read again. **Only a grant still exactly as it was
///   shown is answered**: one that left the project's file, or now reads another way, is
///   nothing, and one that arrived since is not answered by a press that never showed it.
///   What its target works with is part of how it reads ([`arrived_id`]).
/// - **Accept never takes "any persona"** (V100-23): a list that names one is refused whole,
///   here in the command, before anything is audited. Not on my machine may decline it.
/// - Accept leaves out a grant naming a persona the project does not define.
/// - Each answer is audited before it takes effect, as Settings' own are ([`accept`],
///   [`decline`]), and an acceptance is this machine's record only: nothing here writes the
///   project's file.
///
/// Answers the sentence to say where the list had moved.
fn answer_arrival(
    store: &Store,
    ground: &Ground<'_>,
    known: &dyn Fn(&str) -> bool,
    accepted: bool,
    shown: &[String],
    listed: &[String],
) -> Result<Option<String>, String> {
    bring_up_to_date(ground.root, store, ground.audit);
    let now = purlis_core::dispatcharrival::arrival(ground.root).waiting;
    let unseen = now
        .iter()
        .filter(|one| {
            let id = arrived_id_at(ground.root, ground.locks, one);
            !shown.contains(&id) && !listed.contains(&id)
        })
        .count();
    let listed: Vec<_> = now
        .iter()
        .filter(|one| shown.contains(&arrived_id_at(ground.root, ground.locks, one)))
        .collect();
    if accepted && listed.iter().any(|one| one.target == dispatchgrant::ANY) {
        return Err(ANY_IS_SETTINGS.to_owned());
    }
    let mut answered = 0;
    for one in &listed {
        if accepted {
            if one.undefined.is_some() {
                continue;
            }
            accept(store, ground, known, &one.asking, &one.target)?;
        } else {
            decline(ground.root, &one.asking, &one.target, ground.audit)?;
        }
        answered += 1;
    }
    let moved = listed.len() != shown.len() || unseen > 0;
    let did = if accepted { "accepted" } else { "declined" };
    Ok(moved.then(|| {
        if answered == 0 {
            format!(
                "Nothing was {did}: the project's dispatch grants changed after this was \
                 shown. This is what waits now."
            )
        } else {
            format!(
                "The project's dispatch grants changed after this was shown, so only what was \
                 still as shown was {did}. This is what waits now."
            )
        }
    }))
}

/// What the project's dispatch grants ask of the person now (#1506): the grants its file
/// holds that nobody on this machine has accepted or declined, and the ones accepted here
/// that a commit took away. For the Notice the window shows when a teammate's grant arrives.
// On a blocking thread: it settles this machine's acceptances against git's history first.
#[tauri::command]
#[specta::specta]
pub async fn dispatch_arrival(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<DispatchArrival, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window("reading the project's dispatch grants", move || {
        Ok(arrival_of(&root))
    })
    .await
}

/// **Accept** (`accepted`) or **Not on my machine** on the Notice that says a teammate's
/// grant arrived (#1506): `shown` is the ids of the grants to answer, and `listed` the ids
/// of everything the Notice listed. Only a grant still exactly as shown is answered; each is
/// audited first. An accepted pair is in force on this
/// machine from now on and every dispatch waiting on it starts; a declined grant covers
/// nothing here, is not told again, and is changed in Settings. **"Any persona" is never
/// accepted here**: a list naming one is refused. Answers what waits now, and the sentence to
/// say where the list had moved.
// On a blocking thread: accepting settles against git's history.
#[tauri::command]
#[specta::specta]
pub async fn answer_dispatch_arrival(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    accepted: bool,
    shown: Vec<String>,
    listed: Vec<String>,
) -> Result<DispatchArrivalAnswered, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("answering the project's dispatch grants", move || {
        let root = held.root();
        let personas = purlis_core::dispatchdormant::personas_of(root).unwrap_or_default();
        let said = with_ground(&held, |ground| {
            answer_arrival(
                held.dispatch_grants(),
                ground,
                &|name| personas.iter().any(|one| one == name),
                accepted,
                &shown,
                &listed,
            )
        });
        // Told whatever came of it: an answer refused half way may have answered some.
        arrival_moved(&plane);
        Ok(DispatchArrivalAnswered {
            said: said?,
            arrival: arrival_of(root),
        })
    })
    .await
}

/// The person read that the project took away `shown`, grants they had accepted (#1506), by
/// the ids [`DispatchArrival::gone`] gave: each is told once. Nothing is granted or declined.
/// Answers what waits now.
// On a blocking thread: what waits is read after a settling.
#[tauri::command]
#[specta::specta]
pub async fn dispatch_gone_told(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    shown: Vec<String>,
) -> Result<DispatchArrival, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window("recording what was read", move || {
        purlis_core::dispatcharrival::told_gone(&root, &shown)
            .map_err(|why| format!("purlis could not record that it was read: {why}"))?;
        arrival_moved(&plane);
        Ok(arrival_of(&root))
    })
    .await
}

#[cfg(test)]
#[path = "dispatchgrants_tests.rs"]
mod tests;
