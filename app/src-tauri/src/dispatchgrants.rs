//! **The app's half of a dispatch grant** (#1437, spec #1434): the dispatch a chat asked for
//! that no grant covers, held here until the person answers its Notice; the grants made for one
//! chat; and the commands the Notice and Settings press.
//!
//! # The entry point the dispatch core calls
//!
//! [`request_dispatch_grant`]`(held, session, target, brief)`. It answers [`Requested`]:
//! covered (start it, with [`purlis_core::dispatchgrant::grants_for_a_dispatched_chat`]'s
//! answer), needs a grant (nothing starts; the Notice is raised on the asking chat's tab, and
//! the dispatch is held here: the brief, the asking chat from this app's record, the target),
//! locked by policy, or refused. On Allow the held dispatch is handed to whatever
//! [`Store::answers_with`] registered, which is the core's to start.
//!
//! # What a chat cannot do
//!
//! **Nothing a chat sends creates, widens or revokes a grant.** A grant is made by
//! [`allow_dispatch`], a window command the person's press sends, and by nothing on the hook
//! socket: a request carries a target and a brief, and neither is read for anything but the
//! Notice. **The asking persona is this app's record of the chat** ([`asking_of`]), so a
//! request cannot name another's. **The brief is a chat's text**: it is shown inert and capped
//! ([`purlis_core::dispatchgrant::shown_brief`]), apart from purlis's own words. Every Allow
//! and every Revoke is audited before it takes effect.

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
    /// The persona it runs as: its own, or the one a new chat here adopts. None for a chat on
    /// no persona.
    pub persona: Option<String>,
}

/// Chat `session` as `chats` records it, in the project at `root`; none for a chat this app
/// does not have open.
pub fn asking_of(chats: &crate::chats::Chats, root: &Path, session: u32) -> Option<Asking> {
    let chat = chats.recorded_chat(session)?;
    Some(Asking {
        session,
        id: chat.identity.id.clone(),
        name: chats
            .shown_name(session)
            .unwrap_or_else(|| chat.name.clone()),
        persona: chat
            .persona
            .clone()
            .or_else(|| purlis_core::start::persona_for_a_new_chat(root)),
    })
}

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
    /// Writes the audit of a grant or revoke, and answers whether it was written.
    pub audit: Audit<'a>,
    /// Now, in seconds since 1970.
    pub at: u64,
}

impl Store {
    /// Hands every answer to `answers` from now on: the dispatch core's to register.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the dispatch core registers it (#1436)")
    )]
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

    /// The grants in force for `asking` in the project at `root`.
    fn in_force(&self, root: &Path, asking: &Asking) -> InForce {
        InForce::read(root, self.chat_pairs(asking.id.as_deref()))
    }

    /// **A chat asked to dispatch to `target` with `brief`**: covered, held for the person, or
    /// locked. `asking` is the app's record of the chat. Asked twice for the same target while
    /// the first waits, it is the same held dispatch: one Notice, showing the first brief.
    pub fn request(
        &self,
        ground: &Ground<'_>,
        asking: Asking,
        target: &str,
        brief: &str,
    ) -> (Requested, Option<Pending>) {
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
            Covers::NeedsGrant => match self.hold(ground, asking, target, brief, None) {
                Ok((held, new)) => (
                    Requested::NeedsGrant { pending: held.id },
                    new.then_some(held),
                ),
                Err(why) => (Requested::Refused(why), None),
            },
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
        (ground.audit)(
            Some(held.asking.session),
            &dispatchgrant::Audited {
                granted: true,
                asking,
                target: &held.target,
                level,
            },
        )?;
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
                    purlis_core::settings::dispatch::grant(ground.root, pair)?;
                } else {
                    sandbox::local::grant_dispatch(ground.root, &pair.asking, &pair.target)
                        .map_err(|why| format!("purlis could not keep the grant: {why}"))?;
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

    /// **Keep blocked** on the Notice of held dispatch `id`: it goes, nothing is granted, and
    /// the next dispatch across the pair asks again. Answers whether it was held.
    pub fn keep_blocked(&self, id: u32) -> bool {
        let Some(pending) = self.take(id) else {
            return false;
        };
        if pending.locked.is_none() {
            self.answer(&Answered {
                pending,
                allowed: None,
            });
        }
        true
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
    /// The levels Allow is offered at. Empty where policy locks it.
    pub levels: Vec<GrantLevel>,
    /// Where policy locks it: the policy's sentence, naming who set it. No Allow is offered.
    pub locked: Option<String>,
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
        levels,
        locked: held.locked.clone(),
    }
}

/// Seconds since 1970, now.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// **THE DISPATCH CORE'S ENTRY POINT (#1436 calls this; #1437 built it).**
///
/// Chat `session` of `held`'s project asks to dispatch to persona `target` with `brief`. The
/// asking persona is read from this app's record of chat `session`; nothing in the request
/// names it. Answers whether the dispatch is covered, held for the person (the Notice is
/// raised on the asking chat's tab), locked by policy, or refused.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the dispatch core calls it (#1436)")
)]
pub fn request_dispatch_grant(
    held: &crate::planes::Held,
    session: u32,
    target: &str,
    brief: &str,
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
            audit: &|number, audited| held.hooks().record_dispatch_grant(root, number, audited),
            at: now_secs(),
        },
        asking,
        target,
        brief,
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
                    granted: false,
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
                    granted: false,
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

/// The person read the Notice of the project's dispatch grants as it showed them, `shown`: it
/// is not shown again until they change from that. Answers what is still to tell, if anything.
#[tauri::command]
#[specta::specta]
pub fn acknowledge_dispatch_grants(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    shown: Vec<String>,
) -> Result<Option<DispatchGrantsChanged>, String> {
    let held = planes.held(&plane)?;
    dispatchgrant::acknowledge(held.root(), &shown).map_err(|why| why.to_string())?;
    Ok(changed_of(held.root()))
}

#[cfg(test)]
#[path = "dispatchgrants_tests.rs"]
mod tests;
