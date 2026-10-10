//! **The asks registry, as the window reads it** (#1690, spec #1688): everything that waits on
//! the person, one list per project, and the title bar's ✋ counts it.
//!
//! The registry is `purlis_core::harness::asks`: the asks a chat's harness raised (HP-5),
//! first answer wins, and only a human scope answers. This is its window side. It holds no
//! store of its own (I-8): each project's list is **derived, each time it is read**, from the
//! sources that wait today, one adapter each, so an ask leaves the list the moment its source
//! stops waiting, wherever it was answered.
//!
//! - **A permission prompt** a chat or a task holds on its hook ([`permission`]): answered on
//!   that hook, by [`answer_ask`].
//! - **A dispatch grant** the person has not answered ([`dispatch`]): answered by the dispatch
//!   Notice's own commands (`allow_dispatch`, `keep_dispatch_blocked`, `never_dispatch`).
//! - **A sandbox host ask**, a host a chat's sandbox refused ([`sandbox_host`]): answered by
//!   the block Notice's own `allow_sandbox_block`, or put away by [`forget_sandbox_block`].
//! - **A prompt in a harness's own terminal** that purlis holds nothing for ([`terminal`]),
//!   and **a chat waiting on the person's reply** ([`question`]): named, with no choices; the
//!   person goes to the chat. (#1691 detects more of the first.)
//!
//! **Each ask names the existing path that answers it** ([`AnswerPath`]): the window calls that
//! path's own command, checked and audited as it is from its Notice, so the registry opens no
//! route of its own. Every command a path names is the window's alone
//! (`purlis_session_protocol::ui::WINDOW_ONLY`): no chat, and no client but the window, can
//! answer an ask through it, its own least of all (V16).
//!
//! **What an ask says is data.** A chat's names, a brief, a host: each is a string the window
//! draws as text, never as markup or a control, and the choices an ask offers are purlis's
//! fixed words, or for a permission the harness's own option labels.
//!
//! # A chat's permission prompt (HP-6)
//!
//! A Claude Code chat's `PermissionRequest` hook hands its ask to this project's hook channel
//! and waits (`purlis_core::hookwire::permission`). The ask is held in the project's
//! [`HookAsks`], the window is told the asks it now has ([`EVENT`]), and the operator's choice
//! goes back on that hook ([`answer_ask`]), so the harness carries it out without its pane
//! having focus. Unanswered, the hook decides nothing and the pane asks as it always did.
//!
//! **The window answers, and nothing else does.** [`answer_ask`] is a Tauri command the
//! window invokes, admitted as `local-ui`; it is never served on the link to `charterd`
//! (`purlis_session_protocol::ui::WINDOW_ONLY`), and the session protocol refuses its own
//! `answer`. On the hook channel the host reads asks and writes back only the window's choice,
//! and the hook believes a reply only from a listener that is its own ancestor
//! (`purlis_same_user::admit_host`). An answer names its project, chat and ask, and lands only
//! on that chat's ask, once.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use purlis_core::harness::asks::{Admitted, Answerer, Raised};
use purlis_core::harness::hooked::HookAsks;
use purlis_core::harness::model::{Action, ChoiceKind};
use purlis_core::hookwire::Permitting;

use crate::planes::{PlaneId, Planes};

/// The event the window is told a project's asks on: [`Asking`].
pub const EVENT: &str = "asks-changed";

/// Every ask a project holds open, as the window lists them: the whole list each time, so the
/// window never assembles it from events it might have missed one of.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Asking {
    pub plane: PlaneId,
    pub asks: Vec<Shown>,
}

/// One ask, as the registry lists it and its row draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Shown {
    /// The chat that asked.
    pub session: u32,
    /// The ask's key, unique in its project: for a permission its id, which its answer names.
    pub ask: String,
    /// What it asks, in one line, every credential shape masked. Data, never markup.
    pub says: String,
    /// The answers it offers, in the source's order and words. Empty where it is answered
    /// only in its chat ([`AnswerPath::InItsPane`]).
    pub options: Vec<Offered>,
    /// Which source is waiting.
    pub source: AskSource,
    /// Who is asking, by name, the session first and the chat that asked last: `steward 12`,
    /// `#3046 drill`, `log watch` (I-9). From the app's own record of who asked whom, never
    /// from what a chat says of itself; the names are the chats' and are data.
    pub chain: Vec<String>,
    /// The existing path that answers it.
    pub answer: AnswerPath,
}

/// Which source an ask waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum AskSource {
    /// A permission prompt a chat or a task holds on its hook (HP-6).
    Permission,
    /// A dispatch to another persona that no grant covers (#1437).
    Dispatch,
    /// A host a chat's sandbox refused, which an Allow can name (#1342).
    SandboxHost,
    /// A prompt shown in the harness's own terminal that purlis holds nothing for.
    Terminal,
    /// A chat whose turn ended with the next move the person's.
    Question,
}

/// **The existing, checked path an ask is answered by** (#1690): the command the window
/// already answers that source with, and what that command is sent. The registry adds no route.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "kebab-case", tag = "via")]
pub enum AnswerPath {
    /// `answer_ask`, back on the chat's own permission hook, by [`Shown::ask`].
    Hook,
    /// The dispatch Notice's own commands, for held dispatch `id`: `allow_dispatch` at the
    /// level an option names, with `shown`, the digest the question was told; then
    /// `keep_dispatch_blocked` and `never_dispatch`.
    Dispatch { id: u32, shown: String },
    /// The block Notice's own `allow_sandbox_block` at the level an option names, bound to the
    /// block shown; and [`forget_sandbox_block`] for Keep blocked.
    SandboxBlock {
        shown: crate::taskblocks::BlockShown,
    },
    /// Nothing the window can send: the person answers in the chat.
    InItsPane,
}

/// One answer an ask offers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Offered {
    pub id: String,
    pub label: String,
    /// Whether choosing it lets the call run.
    pub allows: bool,
}

/// Told a project's asks each time they change.
pub type Teller = Arc<dyn Fn(Asking) + Send + Sync + 'static>;

/// Where a project's [`Teller`] goes once there is one: a slot, filled after the hook channel
/// is listening, as the project's other listeners are (`hooks::Hooks`).
pub type Telling = Arc<Mutex<Option<Teller>>>;

/// The asks `hooks` holds now, as the window lists them for `plane`.
pub fn asking(plane: &PlaneId, hooks: &HookAsks) -> Asking {
    Asking {
        plane: plane.clone(),
        asks: hooks
            .pending(Instant::now())
            .into_iter()
            .filter_map(permission)
            .collect(),
    }
}

/// **The permission adapter**: a held hook ask as its row draws it, or none for one whose chat
/// is no chat of this project's.
pub fn permission(raised: Raised) -> Option<Shown> {
    let session = raised.chat.parse().ok()?;
    let ask = raised.ask;
    let said = ask.summary.as_str();
    let says = if said.is_empty() {
        match &ask.action {
            Action::Command { line } => format!("Run {line}"),
            Action::Edit { path } => format!("Change {path}"),
            Action::Tool { name, .. } => format!("Use {name}"),
            Action::Elicit { .. } => "Asks you for values".to_owned(),
            Action::Unsaid => "Asks for your permission".to_owned(),
        }
    } else {
        said.to_owned()
    };
    Some(Shown {
        session,
        ask: raised.id.to_string(),
        says: purlis_core::harness::model::Summary::of(&says)
            .as_str()
            .to_owned(),
        options: ask
            .options
            .into_iter()
            .map(|option| Offered {
                allows: option.kind == ChoiceKind::Allow,
                id: option.id,
                label: option.label,
            })
            .collect(),
        source: AskSource::Permission,
        chain: Vec::new(),
        answer: AnswerPath::Hook,
    })
}

/// Tells the window `plane`'s asks, where something listens.
pub fn tell(plane: &PlaneId, hooks: &HookAsks, telling: &Telling) {
    let teller = telling
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(teller) = teller {
        teller(asking(plane, hooks));
    }
}

/// What hears a permission hook's ask on `plane`'s channel: held in `hooks` until it is
/// answered, its hook goes away, or its deadline passes, and the window told each change.
pub fn permitting(plane: PlaneId, hooks: Arc<HookAsks>, telling: Telling) -> Permitting {
    let told = Arc::clone(&hooks);
    purlis_core::hookwire::permission::held_in(
        hooks,
        Arc::new(move || tell(&plane, &told, &telling)),
    )
}

/// The permission asks a project holds open now: what the window lists before any [`EVENT`]
/// arrives.
#[tauri::command]
#[specta::specta]
pub fn pending_asks(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Asking, String> {
    Ok(planes.held(&plane)?.asks())
}

/// **Every ask a project has waiting now** (#1690), from every source ([`every_ask`]): what
/// the window lists and the title bar counts. Read again whenever a source may have moved:
/// [`EVENT`], a chat moving, a dispatch held, a block heard, an answer.
///
/// The window's alone: what waits on the person, dispatches and refused hosts included, is
/// never a link's to read, as `dispatch_grants_needed` is not.
#[tauri::command]
#[specta::specta]
pub fn asks_waiting(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Asking, String> {
    Ok(every_ask(&*planes.held(&plane)?))
}

/// Answers chat `session`'s ask `ask` with its option `option`, as the operator in this window.
///
/// The answer goes back on the channel the ask came on, the chat's own permission hook, and
/// the harness carries it out; nothing is typed into the chat. The first answer wins: one that
/// comes too late, twice, or for another chat's ask is refused with a sentence saying why.
#[tauri::command]
#[specta::specta]
pub fn answer_ask(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    ask: String,
    option: String,
) -> Result<(), String> {
    planes.held(&plane)?.answer_ask(session, &ask, &option)
}

/// The window, as an answerer: a Tauri command is invoked only by the app's own window, which
/// is the `local-ui` scope (ADR 0068 §5).
pub fn the_window() -> Answerer {
    Answerer::admitted(Admitted::LocalUi).expect("the window is a human scope")
}

/// The words an Allow says at each level, as the dispatch and block Notices say them.
fn allow_said(level: crate::sandboxing::GrantLevel) -> &'static str {
    use crate::sandboxing::GrantLevel;
    match level {
        GrantLevel::Chat => "Allow for this chat",
        GrantLevel::You => "Allow for me on this machine",
        GrantLevel::Project => "Allow for everyone in this project",
    }
}

/// The option id an Allow at `level` is sent as: the level's own word, as the Notice sends it.
pub fn level_id(level: crate::sandboxing::GrantLevel) -> &'static str {
    use crate::sandboxing::GrantLevel;
    match level {
        GrantLevel::Chat => "chat",
        GrantLevel::You => "you",
        GrantLevel::Project => "project",
    }
}

/// The option id of Keep blocked, for a dispatch and a sandbox host alike.
pub const KEEP: &str = "keep";
/// The option id of a dispatch's Never for this pair (#1503).
pub const NEVER: &str = "never";

fn allows(levels: &[crate::sandboxing::GrantLevel]) -> impl Iterator<Item = Offered> + '_ {
    levels.iter().map(|level| Offered {
        id: level_id(*level).to_owned(),
        label: allow_said(*level).to_owned(),
        allows: true,
    })
}

fn refuses(id: &str, label: &str) -> Offered {
    Offered {
        id: id.to_owned(),
        label: label.to_owned(),
        allows: false,
    }
}

/// **The dispatch grant adapter** (#1437): a dispatch held for the person, as its Notice was
/// told it. It offers the Notice's answers with no box ticked: Allow at each level the question
/// offers, Keep blocked, and Never for this pair where the asking chat runs as a persona. A
/// dispatch policy locked offers nothing here: its Notice only says so.
pub fn dispatch(told: &crate::dispatchgrants::DispatchPending) -> Shown {
    let says = format!("Wants to hand a task to {}", told.target);
    let options = if told.locked.is_some() {
        Vec::new()
    } else {
        allows(&told.levels)
            .chain([refuses(KEEP, "Keep blocked")])
            .chain(
                told.asking
                    .is_some()
                    .then(|| refuses(NEVER, "Never for this pair")),
            )
            .collect()
    };
    Shown {
        session: told.session,
        ask: format!("dispatch:{}", told.id),
        says: purlis_core::harness::model::Summary::of(&says)
            .as_str()
            .to_owned(),
        answer: if options.is_empty() {
            AnswerPath::InItsPane
        } else {
            AnswerPath::Dispatch {
                id: told.id,
                shown: told.shown.clone(),
            }
        },
        options,
        source: AskSource::Dispatch,
        chain: Vec::new(),
    }
}

/// **The sandbox host adapter** (#1342, #1538): a host chat `session`'s sandbox refused, held
/// now. Allow is offered at each level `locks`, an administrator's policy, leaves open for that
/// host, as its Notice offers it, and Keep blocked. A host the block did not name is typed in
/// its Notice, so it offers nothing here. A folder's block is no host ask.
pub fn sandbox_host(
    session: u32,
    block: &crate::taskblocks::HeldBlock,
    locks: &purlis_core::sandbox::policy::Locks,
) -> Option<Shown> {
    use crate::sandboxing::{GrantLevel, GrantWhat};
    use purlis_core::sandbox::grant::{self, What};
    if block.what != GrantWhat::Host {
        return None;
    }
    let named = (!block.target.is_empty())
        .then(|| grant::host(&block.target).ok())
        .flatten();
    let Some(host) = named else {
        return Some(Shown {
            session,
            ask: format!("block:{session}:{}:{}:", block.operation, block.kind),
            says: "The sandbox refused a host it did not name".to_owned(),
            options: Vec::new(),
            source: AskSource::SandboxHost,
            chain: Vec::new(),
            answer: AnswerPath::InItsPane,
        });
    };
    let levels: Vec<GrantLevel> = [GrantLevel::Chat, GrantLevel::You, GrantLevel::Project]
        .into_iter()
        .filter(|level| {
            locks
                .refuses_grant(&What::Host(host.clone()), (*level).into())
                .is_none()
        })
        .collect();
    let says = format!("The sandbox refused {}", block.target);
    Some(Shown {
        session,
        ask: format!(
            "block:{session}:{}:{}:{}",
            block.operation, block.kind, block.target
        ),
        says: purlis_core::harness::model::Summary::of(&says)
            .as_str()
            .to_owned(),
        options: allows(&levels)
            .chain([refuses(KEEP, "Keep blocked")])
            .collect(),
        source: AskSource::SandboxHost,
        chain: Vec::new(),
        answer: AnswerPath::SandboxBlock {
            shown: crate::taskblocks::BlockShown {
                operation: block.operation.clone(),
                kind: block.kind.clone(),
                what: GrantWhat::Host,
                target: block.target.clone(),
            },
        },
    })
}

/// **The harness-terminal adapter**: chat `session` is stopped on a prompt it asked mid-turn,
/// shown in its own terminal, and purlis holds nothing it could answer it by. Named, so it
/// does not wait unseen; the person answers it in the chat. #1691 detects more of these.
pub fn terminal(session: u32) -> Shown {
    Shown {
        session,
        ask: format!("terminal:{session}"),
        says: "Waiting in its terminal".to_owned(),
        options: Vec::new(),
        source: AskSource::Terminal,
        chain: Vec::new(),
        answer: AnswerPath::InItsPane,
    }
}

/// **The question adapter**: chat `session` is in the needs-you queue with no prompt open,
/// its turn over and the next move the person's. Answered by replying in the chat.
pub fn question(session: u32) -> Shown {
    Shown {
        session,
        ask: format!("question:{session}"),
        says: "Waiting on your reply".to_owned(),
        options: Vec::new(),
        source: AskSource::Question,
        chain: Vec::new(),
        answer: AnswerPath::InItsPane,
    }
}

/// How far up who-asked-whom a chain is followed: far past any depth a dispatch is allowed.
const MOST_CHAIN: usize = 64;

/// **Who is asking, as a chain of names** (I-9): the session first, chat `session` last, by
/// `asker_of`, the app's own record of who asked whom, and `name_of`. A link that loops ends it.
pub fn chain(
    session: u32,
    name_of: &dyn Fn(u32) -> Option<String>,
    asker_of: &dyn Fn(u32) -> Option<u32>,
) -> Vec<String> {
    let named = |chat: u32| name_of(chat).unwrap_or_else(|| format!("chat {chat}"));
    let mut names = vec![named(session)];
    let mut seen = vec![session];
    let mut at = session;
    for _ in 0..MOST_CHAIN {
        match asker_of(at) {
            Some(asker) if !seen.contains(&asker) => {
                seen.push(asker);
                names.push(named(asker));
                at = asker;
            }
            _ => break,
        }
    }
    names.reverse();
    names
}

/// **What waits in one project, read from each source as it stands** (#1690): the inputs of
/// [`derive`], so the registry's rules are tested without a running project.
pub struct Waiting<'a> {
    /// The permission asks its chats' hooks hold open (HP-6).
    pub permissions: Vec<Raised>,
    /// The needs-you queue: the chats the board says need the person.
    pub queue: Vec<u32>,
    /// Whether a chat is stopped on a prompt it asked mid-turn (`Glance::waits_on_its_prompt`).
    pub at_its_prompt: &'a dyn Fn(u32) -> bool,
    /// The dispatches held for the person, as their Notices are told them.
    pub dispatches: Vec<crate::dispatchgrants::DispatchPending>,
    /// The blocks each chat is held on now.
    pub blocks: Vec<(u32, crate::taskblocks::HeldBlock)>,
    /// The administrator's policy, which the levels an Allow is offered at answer to.
    pub locks: &'a purlis_core::sandbox::policy::Locks,
    /// The name a chat is shown under, where it is open.
    pub name_of: &'a dyn Fn(u32) -> Option<String>,
    /// The chat that asked for a chat as a task, by the app's own record.
    pub asker_of: &'a dyn Fn(u32) -> Option<u32>,
}

/// **The asks of one project, one per thing that waits** (#1690): each source's adapter, then
/// each ask's chain. A chat in the needs-you queue is no ask of its own where a structured ask
/// of its says why it waits: its held permission prompt is that prompt, and a held dispatch is
/// what its turn waits on. The rest of the queue is a prompt in its terminal or a question.
pub fn derive(waiting: &Waiting<'_>) -> Vec<Shown> {
    let mut asks: Vec<Shown> = waiting
        .permissions
        .iter()
        .cloned()
        .filter_map(permission)
        .collect();
    asks.extend(waiting.dispatches.iter().map(dispatch));
    let said: std::collections::HashSet<u32> = asks.iter().map(|ask| ask.session).collect();
    asks.extend(
        waiting
            .queue
            .iter()
            .filter(|session| !said.contains(session))
            .map(|&session| {
                if (waiting.at_its_prompt)(session) {
                    terminal(session)
                } else {
                    question(session)
                }
            }),
    );
    asks.extend(
        waiting
            .blocks
            .iter()
            .filter_map(|(session, block)| sandbox_host(*session, block, waiting.locks)),
    );
    for ask in &mut asks {
        ask.chain = chain(ask.session, waiting.name_of, waiting.asker_of);
    }
    asks
}

/// The chat that asked for chat `session` as a task, as `chats` records it while it is open.
fn asker_in(chats: &crate::chats::Chats, session: u32) -> Option<u32> {
    let from = chats.recorded_chat(session)?.from?;
    (from.mode == purlis_core::reopen::Mode::Task).then_some(from.chat)
}

/// Every ask project `held` has waiting now, as the window lists them.
pub fn every_ask(held: &crate::planes::Held) -> Asking {
    let plane = held.plane_id();
    let root = held.root();
    let locks = purlis_core::sandbox::policy::Locks::of(root);
    let board = held.board();
    let chats = held.chats();
    // The queue is the board's, whichever chat it is read through.
    let queue = chats
        .open_now()
        .first()
        .map(|any| board.now(any.session).queue)
        .unwrap_or_default();
    let store = held.dispatch_grants();
    let waiting = Waiting {
        permissions: held.hooks().asks().pending(Instant::now()),
        queue,
        at_its_prompt: &|session| board.glance(session).waits_on_its_prompt(),
        dispatches: store
            .every_waiting()
            .iter()
            .map(|one| store.told(plane, root, &locks, one))
            .collect(),
        blocks: chats.blocks().every(),
        locks: &locks,
        name_of: &|session| chats.shown_name(session),
        asker_of: &|session| asker_in(chats, session),
    };
    Asking {
        plane: plane.clone(),
        asks: derive(&waiting),
    }
}

/// **Keep blocked on a sandbox host ask** (#1690): chat `session` is no longer held on the
/// block shown, so the registry stops listing it. It grants nothing and changes no sandbox; it
/// is the Notice's Keep blocked, told to the app, which before kept the block until the chat
/// ended. Answers whether the block was still held.
#[tauri::command]
#[specta::specta]
pub fn forget_sandbox_block(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    shown: crate::taskblocks::BlockShown,
) -> Result<bool, String> {
    let held = planes.held(&plane)?;
    let blocks = held.chats().blocks();
    Ok(
        match crate::taskblocks::shown_one(blocks, session, &shown) {
            Ok(block) => {
                blocks.answered(session, &block);
                true
            }
            Err(_) => false,
        },
    )
}

#[cfg(test)]
#[path = "asking_tests.rs"]
mod tests;
