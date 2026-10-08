//! The app's side of the hook channel: what a chat is doing, and which one needs you.
//!
//! A hook writes one line on a socket this owns (`purlis_core::hookwire`), which moves a
//! chat on the board and, if a reader would see a difference, tells the window. There is no
//! polling anywhere: the listener blocks on `accept`, and the window is pushed to.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use purlis_core::hookwire::{
    Answer, Ask, ChatTokens, Hearing, Listener, NOTHING_ANSWERS, Reading, Report, SessionSaved,
    StartedByHand,
};
use purlis_core::session::Exit;
use purlis_core::state::{Board, State};

use purlis_core::harness::asks::Asks;
use purlis_core::harness::hooked::HookAsks;

use crate::host::{ChatBoard, Glance};
use crate::planes::{PlaneId, Teller};

/// The event the window listens for. One chat, its state, whether it is asking for you.
///
/// **The plane travels with it, and that is not decoration.** Every plane numbers its chats
/// from one, so a window holding two of them would be told "session 3 is waiting" twice about
/// two different chats. The pair is the identity; one half of it is a guess.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Moved {
    pub plane: PlaneId,
    pub session: u32,
    pub state: String,
    pub needs_you: bool,
    /// Every chat asking for you, so the queue is never assembled from a series of events
    /// the window might have missed one of.
    pub queue: Vec<u32>,
    /// When this chat last moved, as a count of moves on every plane's board in this process
    /// — bigger is more recent, within a plane and across planes.
    /// `purlis_core::state::Board::moved_at` is the whole definition.
    ///
    /// **The window cannot work this out for itself, which is why it rides an event that
    /// already fires.** Charter ADR 0039 sorts the chat strip's overflow menu by last
    /// activity, and nothing in the window knows when a chat did anything: the strip is
    /// `tabs.order`, which is an opening order, and the needs-you queue is oldest-first,
    /// which is a different fact. An order computed in the window would also restart at
    /// every launch and disagree between two windows on one plane, where this one is the
    /// board's and the board is the plane's.
    ///
    /// A count rather than a clock, and a `u32` rather than a `u64`: both are argued where
    /// the field is produced, and the second is not negotiable here — `specta` refuses to
    /// export a `u64` and the app panics at startup in a debug build when one is reached
    /// for.
    pub moved_at: u32,
    /// The chats that have reported back to this one and not been read yet, by the name the
    /// operator sees them under, oldest first (charter-app#259). No needs-you item of its own
    /// (#1448): a chat in the queue for another reason says `<child> reported back` there.
    /// Empty for nearly every chat, and emptied by this chat's next prompt, which is the turn
    /// the reports are handed.
    pub reports: Vec<String>,
    /// The commits of this chat charter's `pre-commit` refused and the operator has not seen,
    /// each as the one masked line its item says, oldest first (SQ-16). Emptied by the chat's
    /// next prompt, or by Ignore.
    pub refusals: Vec<String>,
    /// The chats this one started that the operator stopped, by name, oldest first (#1448):
    /// its row says `<child> was stopped`. purlis's own word, apart from `reports`, which are
    /// what chats said. `null` for nearly every chat, and emptied as `reports` is.
    #[specta(optional)]
    pub stopped: Option<Vec<String>>,
    /// Why this chat needs the person when no hook of its own said so, oldest first (#1448):
    /// its needs-you item says the latest. `null` for nearly every chat, which needs the person
    /// for nothing of the kind, and emptied by the chat's next prompt, or by Ignore.
    #[specta(optional)]
    pub needs: Option<Vec<Need>>,
    /// Which snapshot of the board this is — bigger was taken later (charter-app#248).
    ///
    /// **What lets the window put its events back in order.** Every `Moved` is built under the
    /// board's lock, but it is SENT after the lock is let go, on whichever thread built it: a
    /// hook's report on the socket's thread, a close on the command's. So a report taken just
    /// before a close can reach the window just after it, and the window, which keeps the last
    /// queue it was told, would put the closed chat back. The window drops any snapshot older
    /// than the one it holds (`chatState.ts`), which it can do only because this is numbered
    /// in the order the board was read. [`sequence`] is the whole definition.
    pub sequence: u32,
    /// The child agents of this chat's current run, oldest first (FD-18, W8): each sub-agent
    /// or child its harness spawned, drawn under the chat. Empty for nearly every chat.
    pub children: Vec<ChildAgent>,
    /// Whether this move is only what a chat it started did: a report that landed, or a stop
    /// below it (#1491). The window is told as of any move; nothing interrupts the person for
    /// it ([`Moved::interrupts`]). The app's own fact, and never sent.
    #[serde(skip)]
    #[specta(skip)]
    pub counts_only: bool,
    /// Whether this move raised something for the person (#1491): the chat came into the
    /// queue, or a reason was added to one already in it (`purlis_core::state::raised`). A
    /// chat that moves while its item stands raised nothing. The app's own fact, and never
    /// sent.
    #[serde(skip)]
    #[specta(skip)]
    pub raised: bool,
}

impl Moved {
    /// **Whether this move is one to interrupt the person for** (#1491, V100-15): the chat it
    /// is about is a needs-you item, and the move is its own. A task that finished changes
    /// what its asking chat's row counts, and is told as a move of that chat; it is no reason
    /// to send a system notification, whether or not that chat is already an item for
    /// something else. A task that failed is an item on the asking chat, raised as a move of
    /// its own, and that one does interrupt.
    ///
    /// **On the rising edge only**: a failure's item stands across the asking chat's own
    /// turns, and each prompt, stop and helper of that chat is a move with the item still
    /// standing. One failure is one notification.
    ///
    /// Whether the person is already looking at the chat is asked after this, and is not this
    /// rule's.
    pub fn interrupts(&self) -> bool {
        self.needs_you && self.raised && !self.counts_only
    }

    /// This move, as one that only changes what the chat's row counts.
    #[must_use]
    pub fn counting_only(mut self) -> Self {
        self.counts_only = true;
        self
    }
}

/// Why a chat needs the person, as the window says it (`purlis_core::state::Need`, #1448). A
/// dispatch grant that is needed is the next kind (#1437).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Need {
    /// Its report has nowhere to go: the chat that asked for it, `asker`, has gone.
    ReportUndelivered { asker: String },
    /// A task it asked for, `task`, came to nothing (#1491), and `why` says why in a few
    /// words, where anything does. `id` names this failure and no other: its dispatch
    /// record's id, which its finished row carries. `chat` is the task's chat while it is
    /// still open. Not emptied by the chat's next prompt: by the person's look at it, their
    /// Ignore, or the task's row being cleared. Held in memory only: a restart of the app
    /// keeps the task's finished row and not this item.
    TaskFailed {
        id: String,
        #[specta(optional)]
        chat: Option<u32>,
        task: String,
        how: HowFailed,
        why: String,
    },
}

/// How a task came to nothing (`purlis_core::state::HowFailed`), as the window is sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HowFailed {
    /// It reported that it failed, or that it was blocked.
    Failed,
    /// Its program ended while it still owed its report.
    Unreported,
    /// It was asked for and never started.
    DidNotStart,
}

impl From<purlis_core::state::FailedTask> for Need {
    fn from(failed: purlis_core::state::FailedTask) -> Self {
        use purlis_core::state::HowFailed as How;
        Self::TaskFailed {
            id: failed.id,
            chat: failed.chat,
            task: failed.task,
            how: match failed.how {
                How::Failed => HowFailed::Failed,
                How::Unreported => HowFailed::Unreported,
                How::DidNotStart => HowFailed::DidNotStart,
            },
            why: failed.why,
        }
    }
}

impl From<purlis_core::state::Need> for Need {
    fn from(need: purlis_core::state::Need) -> Self {
        match need {
            purlis_core::state::Need::ReportUndelivered { asker } => {
                Self::ReportUndelivered { asker }
            }
        }
    }
}

/// One child agent of a chat, as the window draws it under the chat (ADR 0066, ADR 0076 §6).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ChildAgent {
    /// The harness's id for it, unique within the chat.
    pub agent: String,
    /// `running` while it works, `done` once its own stop is heard, or the word its chat
    /// ended as when the chat ended first: a stop of the chat stops its children.
    pub state: String,
}

/// The event the window is sent when a harness was started by hand in a shell tab (ADR 0062).
pub const BY_HAND: &str = "harness-by-hand";

/// A harness the operator started by hand in a shell tab, as the window draws its banner.
///
/// **Nothing about the chat moves.** It is not a state and not a needs-you item: the tab says
/// what happened and offers to open that harness as a chat, and the operator's click is what
/// does anything.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ByHand {
    pub plane: PlaneId,
    /// The shell tab's chat.
    pub session: u32,
    /// The harness, by the word the plane calls it — a profile's `kind`.
    pub harness: String,
    /// Where the shell was standing when it started it, which is where a chat opened in its
    /// place starts.
    pub cwd: Option<String>,
}

/// Told when a harness is started by hand in a shell tab of any plane. The event carries its
/// plane, as [`Moved`] does.
pub type ByHandTeller = Arc<dyn Fn(ByHand) + Send + Sync + 'static>;

/// What the window is told about `notice`, heard on `plane`'s socket — or nothing, for a
/// harness this app does not start. A word charter has no chat for would put a button on the
/// tab that could only be refused.
fn by_hand(plane: &PlaneId, notice: StartedByHand) -> Option<ByHand> {
    let harness = purlis_core::harness::Harness::of_kind(&notice.started_by_hand)?;
    Some(ByHand {
        plane: plane.clone(),
        session: notice.chat,
        harness: harness.name().to_owned(),
        cwd: notice.cwd.map(|cwd| cwd.display().to_string()),
    })
}

/// The board, the socket, and the thread reading it — one plane's whole side of the channel.
pub struct Hooks {
    /// The plane this listens for, stamped onto every event it sends.
    plane: PlaneId,
    /// Shared with the thread reading the socket: one board, so what the window is told and
    /// what the window can ask for can never disagree.
    board: Arc<Mutex<Board>>,
    /// The reading, while it is going on. Held in a lock rather than owned outright so that
    /// closing a plane can release the socket THEN, rather than whenever the last handle to
    /// the plane happens to be dropped — a project the operator closed has to stop listening
    /// while they are still looking at the app.
    reading: Mutex<Option<Reading>>,
    socket: Option<PathBuf>,
    /// The chats' tokens the socket checks every line against, issued as each chat starts
    /// (`crate::sessions::Reporting`).
    tokens: Option<Arc<ChatTokens>>,
    /// Who answers an ask on this socket, once there is someone to (charter-app#204).
    ///
    /// A slot filled after the fact, because the answer needs the plane's chats and the
    /// chats are built after the socket: a session's environment carries the socket's path,
    /// so the socket has to exist first. Until it is filled every ask is answered with a
    /// refusal, never with a silence an asker would have to wait out.
    answering: Arc<Mutex<Option<Answering>>>,
    /// Told each report the board took, after the window has been told what it moved — a
    /// slot filled after the fact, for `answering`'s reason: what listens needs the plane's
    /// chats, which are built after the socket. A curation chat's typed prompt waits on it
    /// (`crate::curation::Typed`).
    heard: Arc<Mutex<Option<Heard>>>,
    /// Told each time the board moves a chat onto another conversation — a slot filled after
    /// the fact, for `answering`'s reason. The chats' record listens, so the conversation a
    /// relaunch resumes is the one the chat is in now (Q10).
    following: Arc<Mutex<Option<Following>>>,
    /// The host's event log, once the app has one (FD-9) — a slot filled after the fact, so
    /// a project is opened the same way with or without it. Every line the channel hears from
    /// a chat's hooks is recorded there, whether or not it moved the board.
    events: Arc<Mutex<Option<Events>>>,
    /// Told EVERY report on this socket once the board has had it, whether it moved the board or
    /// not — a slot filled after the fact, for `answering`'s reason. A smart close queued for a
    /// turn's end waits on it (`crate::smartclose`): the `Stop` that ends a turn in which the
    /// chat asked a question moves nothing a reader sees, and is still the end of the turn.
    all_reports: Arc<Mutex<Option<Heard>>>,
    /// Asked what a chat waits on as its turn's end is applied (#1491): the tasks below it and
    /// the chat that dispatched it, which the project's records know and the board does not.
    /// A slot filled after the fact, for `answering`'s reason; until it is, a chat waits on
    /// nothing and every end of turn is the person's.
    waits: Arc<Mutex<Option<WaitsOf>>>,
    /// Told each session record a chat's `charter session record` says it saved (ADR 0064) — a
    /// slot filled after the fact, for `answering`'s reason.
    saved: Arc<Mutex<Option<SavedHeard>>>,
    /// The permission asks this project's chats hold open on their hooks (HP-6, `asking`).
    asks: Arc<HookAsks>,
    /// Told this project's asks each time they change — a slot filled after the fact, for
    /// `answering`'s reason.
    asks_told: crate::asking::Telling,
    /// Told each path a chat's file tool touched (FM-6), as the chat said it — a slot filled
    /// after the fact, for `answering`'s reason: confining it needs the chat's folder.
    /// **Never recorded**: nothing here writes it, and the event log is not told (D-86a).
    touching: Arc<Mutex<Option<Touches>>>,
    /// Told each sandbox block a chat's hook found (#1338), once it is kept for `purlis doctor`
    /// — a slot filled after the fact, for `answering`'s reason.
    blocked: Arc<Mutex<Option<Blocks>>>,
    /// Runs each brokered `secret exec` a sandboxed chat asks for (#1407) — a slot filled after
    /// the fact, for `answering`'s reason: it needs the chat's record. Empty, the asker is told
    /// nothing answers and runs the command itself.
    secret_exec: Arc<Mutex<Option<SecretExecs>>>,
}

/// What runs a brokered `secret exec` (#1407): the ask, and the connection it is held on.
pub type SecretExecs = Arc<
    dyn Fn(
            purlis_core::secrets::brokered::Ask,
            Box<dyn std::io::BufRead + Send>,
            Box<dyn std::io::Write + Send>,
        ) + Send
        + Sync
        + 'static,
>;

/// What is told each sandbox block a chat's hook found, as the hook sent it.
pub type Blocks = Arc<dyn Fn(purlis_core::hookwire::SandboxBlocked) + Send + Sync + 'static>;

/// The event the window is sent when a chat's sandbox blocked an operation (#1338).
pub const SANDBOX_BLOCKED: &str = "chat-sandbox-blocked";

/// A sandbox block, as the window shows it on the chat's tab (#1338): an operation and the kind
/// of path or host, by their fixed words, and the sentence purlis says about them; and what the
/// Notice can offer for it (#1342): Allow for a host or a folder, with what it would name, or
/// the way that works for what is never granted. **No argument or output**.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ChatBlocked {
    pub plane: PlaneId,
    pub session: u32,
    /// The operation's word: `write`, `read`, `connect`, ….
    pub operation: String,
    /// The kind's word: `project-files`, `toolchain-cache`, `host`, ….
    pub kind: String,
    /// Whether it was purlis's own operation: a purlis bug, which the Notice offers to report.
    pub ours: bool,
    /// The harness the chat runs, by the word the project calls it, when it is one purlis
    /// starts.
    pub harness: Option<String>,
    /// What was blocked, as a sentence names it: "a write to the project's own files".
    pub said: String,
    /// What the Notice offers (#1342).
    pub offer: BlockOffer,
    /// What Allow would name, shown whole before anyone presses it: the host the report named,
    /// or the folder a refused write was in. Null for a host the report did not name, which the
    /// person types. The app checks it again before anything is granted.
    pub target: Option<String>,
    /// For [`BlockOffer::Brokered`], the way that works instead; for
    /// [`BlockOffer::Unsandboxed`], why purlis grants nothing here; for [`BlockOffer::Policy`],
    /// what policy forbids, naming the policy and who set it (#1343).
    pub route: Option<String>,
    /// For [`BlockOffer::Host`] and [`BlockOffer::Write`], the levels Allow may keep it at:
    /// each a policy does not forbid (#1343).
    pub levels: Vec<crate::sandboxing::GrantLevel>,
}

/// What the app does with a sandbox block chat `block.chat`'s hook sent on the project at
/// `root` (#1338), heard at `at` (seconds since 1970, and now): let through the chat's
/// [`purlis_core::sandboxblock::Throttle`], then kept for `purlis doctor`'s count, then handed
/// to whoever `slot` holds for the chat's notice. A block the throttle holds back is neither
/// kept nor shown. It holds an operation and a kind only, so keeping it keeps nothing of what
/// the chat ran. The listener is taken out of the lock before it runs, as an answer is.
fn heard_block(
    root: &Path,
    throttle: &Mutex<purlis_core::sandboxblock::Throttle>,
    slot: &Mutex<Option<Blocks>>,
    block: purlis_core::hookwire::SandboxBlocked,
    at: (u64, std::time::Instant),
) {
    let let_through = throttle
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .lets(block.chat, &block.sandbox_blocked, at.1);
    if !let_through {
        return;
    }
    if let Err(why) = purlis_core::sandboxblock::record(root, &block.sandbox_blocked, at.0) {
        tracing::warn!(
            "purlis: a sandbox block of chat {} was not kept for the doctor's count ({why})",
            block.chat
        );
    }
    let listener = slot.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(listener) = listener {
        listener(block);
    }
}

/// Told each sandbox block, as the window shows it.
pub type BlockTeller = Arc<dyn Fn(ChatBlocked) + Send + Sync + 'static>;

/// What the window is told of a block a chat's hook sent on `plane`'s socket. A harness word the
/// app does not start is no harness: the line is the chat's own.
pub fn blocked(plane: &PlaneId, blocked: &purlis_core::hookwire::SandboxBlocked) -> ChatBlocked {
    let block = blocked.sandbox_blocked;
    let (offer, target, route, levels) = held(
        &purlis_core::sandbox::policy::Locks::of(plane.root()),
        offered(plane.root(), &block, blocked.target.as_deref()),
    );
    ChatBlocked {
        offer,
        target,
        route,
        levels,
        plane: plane.clone(),
        session: blocked.chat,
        operation: block.operation.word().to_owned(),
        kind: block.kind.word().to_owned(),
        ours: block.ours,
        harness: blocked
            .harness
            .as_deref()
            .and_then(purlis_core::harness::Harness::of_kind)
            .map(|harness| harness.name().to_owned()),
        said: block.said(),
    }
}

/// What a block's Notice offers (#1342).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum BlockOffer {
    /// Allow a host.
    Host,
    /// Allow a folder to write.
    Write,
    /// Never granted, and there is a way that works: [`ChatBlocked::route`] says it.
    Brokered,
    /// Not something purlis grants a chat (off the allowlist, a local socket): the person may
    /// start this chat again without the sandbox; [`ChatBlocked::route`] says why.
    Unsandboxed,
    /// Nothing to allow: purlis's own operation (a bug to report), or a block with nothing a
    /// grant could name.
    None,
    /// What would be offered is forbidden by policy, and so is starting the chat without the
    /// sandbox (#1343): nothing is offered, and [`ChatBlocked::route`] names the policy and who
    /// set it, so the person knows whom to ask.
    Policy,
}

/// What a block's Notice offers once `locks`, an administrator's policy (#1343), have their say
/// on what [`offered`] would offer: the strictest wins.
///
/// - Allow is offered only at the levels policy leaves open ([`ChatBlocked::levels`]); a host
///   named whole is judged as it would be kept at each level.
/// - Where policy leaves no Allow, or forbids what a Start without the sandbox stood in for,
///   the Notice offers Start without the sandbox, saying why, unless policy forbids that too:
///   then it offers nothing and says why ([`BlockOffer::Policy`]). It never dead-ends silently.
fn held(
    locks: &purlis_core::sandbox::policy::Locks,
    (offer, target, route): (BlockOffer, Option<String>, Option<String>),
) -> (
    BlockOffer,
    Option<String>,
    Option<String>,
    Vec<crate::sandboxing::GrantLevel>,
) {
    use crate::sandboxing::GrantLevel;
    use purlis_core::sandbox::grant::{self, What};
    let instead = |target: Option<String>, why: String| match locks.opt_out_refused() {
        None => (BlockOffer::Unsandboxed, target, Some(why), Vec::new()),
        Some(_) => (
            BlockOffer::Policy,
            target,
            Some(format!(
                "{why} Policy forbids starting this chat without the sandbox too."
            )),
            Vec::new(),
        ),
    };
    match offer {
        BlockOffer::Host => {
            let named = target.as_deref().and_then(|typed| grant::host(typed).ok());
            let mut why = None;
            let levels: Vec<GrantLevel> = [GrantLevel::Chat, GrantLevel::You, GrantLevel::Project]
                .into_iter()
                .filter(|level| {
                    let refused = match &named {
                        Some(host) => {
                            locks.refuses_grant(&What::Host(host.clone()), (*level).into())
                        }
                        // A host the person types is judged once typed; here only the level.
                        None => (locks.forbids_personal_hosts() && *level != GrantLevel::Project)
                            .then(|| {
                                format!("Policy forbids hosts of your own. {}", locks.locked_by())
                            }),
                    };
                    if refused.is_some() && why.is_none() {
                        why = refused.clone();
                    }
                    refused.is_none()
                })
                .collect();
            match (levels.is_empty(), why) {
                (true, Some(why)) => instead(target, why),
                _ => (offer, target, route, levels),
            }
        }
        BlockOffer::Write => match locks.write_grants_refused() {
            Some(why) => instead(target, why),
            None => (
                offer,
                target,
                route,
                vec![GrantLevel::Chat, GrantLevel::You],
            ),
        },
        BlockOffer::Unsandboxed => match locks.opt_out_refused() {
            Some(locked) => (
                BlockOffer::Policy,
                target,
                Some(
                    format!("{} {locked}", route.unwrap_or_default())
                        .trim()
                        .to_owned(),
                ),
                Vec::new(),
            ),
            None => (offer, target, route, Vec::new()),
        },
        BlockOffer::Brokered | BlockOffer::None | BlockOffer::Policy => {
            (offer, target, route, Vec::new())
        }
    }
}

/// What a block's Notice offers (#1342) for `block`, refused on `target` as the hook read it, in
/// the project at `root`: the offer, what Allow would name, and the sentence for one that is not
/// Allow.
///
/// - purlis's own operation is a purlis bug: Report, never Allow.
/// - A refused host: Allow, naming it where the report named one concrete host. A wildcard on
///   the line is never proposed (review S3): a real refusal names one host, and a person can
///   still type a wildcard in Settings.
/// - A refused write: Allow on the folder it was in, shown whole, where that folder is on the
///   allowlist ([`purlis_core::sandbox::grant::write`], D-1342-10); a denial class gets the way
///   that works; anything else gets "Start without the sandbox".
/// - purlis's state, a protected file and any read (reads are denied only for the classes no
///   person grants): the brokered route. A local socket: "Start without the sandbox".
fn offered(
    root: &Path,
    block: &purlis_core::sandboxblock::Block,
    target: Option<&str>,
) -> (BlockOffer, Option<String>, Option<String>) {
    use purlis_core::sandbox::{Class, grant};
    use purlis_core::sandboxblock::{Kind, Operation};
    if block.ours {
        return (BlockOffer::None, None, None);
    }
    let brokered = |class| {
        (
            BlockOffer::Brokered,
            None,
            Some(grant::brokered_route(class).to_owned()),
        )
    };
    match (block.operation, block.kind) {
        (Operation::Connect, Kind::Host) => (
            BlockOffer::Host,
            target
                .and_then(|typed| grant::host(typed).ok())
                .map(|host| host.to_string())
                .filter(|host| !host.starts_with("*.")),
            None,
        ),
        (Operation::Connect, Kind::LocalSocket) => (
            BlockOffer::Unsandboxed,
            None,
            Some(
                "purlis will not let a sandboxed chat reach a local socket other than its own: \
                 one could be any service on this machine."
                    .to_owned(),
            ),
        ),
        (_, Kind::ProjectState) => brokered(Class::Integrity),
        (_, Kind::ProtectedFile) => brokered(Class::LaterCode),
        (Operation::Read, _) => brokered(Class::Vaults),
        (
            Operation::Write | Operation::File,
            Kind::ProjectFiles | Kind::Home | Kind::ToolchainCache | Kind::System,
        ) => {
            let Some(path) = target.filter(|path| Path::new(path).is_absolute()) else {
                return (BlockOffer::None, None, None);
            };
            let folder = grant::proposed_folder(Path::new(path))
                .display()
                .to_string();
            let ground = grant::Ground::of(root, root, &purlis_core::sandbox::Machine::this());
            match grant::write(&folder, &ground.place()) {
                Ok(folder) => (BlockOffer::Write, Some(folder.display().to_string()), None),
                Err(grant::Refused::Never(route)) => (BlockOffer::Brokered, None, Some(route)),
                Err(grant::Refused::Outside(why) | grant::Refused::Not(why)) => {
                    (BlockOffer::Unsandboxed, Some(folder), Some(why))
                }
            }
        }
        _ => (BlockOffer::None, None, None),
    }
}

/// What is told each path a chat's file tool touched, unconfined.
pub type Touches = Arc<dyn Fn(purlis_core::hookwire::Touching) + Send + Sync + 'static>;

/// The event the window is sent when a chat's file tool touches a file (FM-6).
pub const TOUCHING: &str = "chat-touching";

/// A file a chat's tool touched, as the window marks it in the tree for a few seconds (FM-6,
/// #1109). It travels in memory only and is never written anywhere (D-86a).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ChatTouching {
    pub plane: PlaneId,
    pub session: u32,
    /// The path inside the chat's own folder, `/`-separated, with no `..` and no `.git`:
    /// confined by `purlis_core::touching::confine` before it is sent.
    pub path: String,
}

/// Told each file a chat touched, once confined and let through the rate gate.
pub type TouchTeller = Arc<dyn Fn(ChatTouching) + Send + Sync + 'static>;

/// What the window is told of a path chat `touched.chat` said its tool touched, or nothing.
///
/// **Nothing unless it is inside the chat's own folder** (`folder`, where it works): confined
/// by core, so a chat can put no marker outside its branch. Nothing either once the chat has put
/// up its share this second, or for the same path again at once (`gate`), so no chat can flood
/// the window. A chat with no folder known marks nothing.
pub fn touched(
    plane: &PlaneId,
    folder: Option<&Path>,
    gate: &Mutex<purlis_core::touching::Gate>,
    touched: &purlis_core::hookwire::Touching,
    now: std::time::Instant,
) -> Option<ChatTouching> {
    let path = purlis_core::touching::confine(folder?, &touched.touching)?;
    gate.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .lets(touched.chat, &path, now)
        .then(|| ChatTouching {
            plane: plane.clone(),
            session: touched.chat,
            path,
        })
}

/// What is told a session record was saved.
pub type SavedHeard = Arc<dyn Fn(SessionSaved) + Send + Sync + 'static>;

/// What is told each report the board took.
pub type Heard = Arc<dyn Fn(&Report) + Send + Sync + 'static>;

/// What chat `session` waits on that is not the person, by the project's own records
/// (`purlis_core::state::Waits`, #1491): asked as a report that can end its turn is applied.
/// The second argument says the report is that turn's end, and not the nudge after one.
pub type WaitsOf = Arc<dyn Fn(u32, bool) -> purlis_core::state::Waits + Send + Sync + 'static>;

/// What is told that chat `session` is now in conversation `id`: the id its own harness
/// reported, that the board adopted or followed — and, where the move began a run (`/clear`,
/// ADR 0066), that run's id, which is the chat's current run from now on.
pub type Following = Arc<dyn Fn(u32, &str, Option<&str>) + Send + Sync + 'static>;

/// What answers an ask, told which connection it came on.
pub type Answering = Arc<dyn Fn(u64, Ask) -> Answer + Send + Sync + 'static>;

/// The host's event log, shared by every project this process holds: one writer per device.
pub type Events = Arc<Mutex<purlis_core::eventlog::Recorder>>;

/// Where this app listens, and where containment of that path begins.
///
/// The two travel together because `Listener::bind` needs both: a link anywhere between them
/// is refused, and the root is named rather than worked out — a walk from the root of the
/// filesystem would refuse every path on macOS, where `/tmp` and `/var` are themselves links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Where {
    /// A directory the caller already trusts. Nothing above it is checked.
    pub within: PathBuf,
    pub socket: PathBuf,
}

/// Where this app listens for its sessions' hooks.
///
/// Beside the record M1.7 already writes (`.charter/app/`) when there is a plane, so
/// everything the app keeps for a plane is in one place and an operator looking for it finds
/// it. `Listener::bind` makes that directory 0700.
///
/// **There is always somewhere.** The channel belongs to the APP, not to the plane: a chat
/// started outside a plane is still a chat, and it would be a strange rule that a harness can
/// report what it is doing only when there happens to be a `charter.toml` above it. An
/// earlier version answered `None` here, which quietly made every chat in such a run
/// `unknown` — including every chat in the scenario tests, which is how it was found.
///
/// The fallback is keyed by the plane (or by nothing) so two apps do not land on one socket,
/// and by the user so two accounts on one machine do not either. A path is handed to each
/// session in its environment, so nothing ever has to guess it.
pub fn socket_for(plane: Option<&Path>) -> Where {
    if let Some(plane) = plane {
        let beside_the_record = purlis_core::names::state(plane)
            .join("app")
            .join("hooks.sock");
        // macOS allows 104 bytes for a unix socket path including the terminator
        // (`sys/un.h`), Linux 108; the smaller is the one to hold to, since a plane is
        // portable. A plane nested deeper than that is not a failure, just not somewhere the
        // socket can live.
        if beside_the_record.as_os_str().len() <= LONGEST_SOCKET_PATH {
            // The plane is the root: it is the operator's own tree, and `.charter/app` below
            // it is charter's own directory (charter-app#28 rules the same for the record
            // that already lives there).
            return Where {
                within: plane.to_path_buf(),
                socket: beside_the_record,
            };
        }
    }
    let within = private_dir();
    let socket = within
        .join(format!("charter-{}-{:016x}", whoami(), keyed_on(plane)))
        .join("hooks.sock");
    Where { within, socket }
}

/// Where a socket goes when it cannot go beside the plane.
///
/// `$XDG_RUNTIME_DIR` first: on Linux it is the standard per-user 0700 directory for exactly
/// this, and the temp directory there is `/tmp`, which everyone can write to. macOS has no
/// such variable and its `TMPDIR` is already a per-user 0700 directory.
///
/// Either way `Listener::bind` creates the socket's own directory with the mode set as it is
/// made and refuses one it does not own, so this chooses a good neighbourhood rather than
/// being the thing that keeps anyone out.
fn private_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute() && dir.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// The longest a unix socket path may be, on the stricter of the two platforms charter builds
/// for. One byte is left for the terminator.
const LONGEST_SOCKET_PATH: usize = 103;

/// Something short and stable that differs between users on one machine.
///
/// `TMPDIR` is already per-user on macOS and is not on Linux, so this is what keeps two
/// accounts apart there. It is not a secret and is not relied on to be one — the directory's
/// 0700 is what keeps others out.
fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .filter(|user| {
            user.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .unwrap_or_else(|| "charter".to_owned())
}

/// A short, stable name for a plane — or for having none.
fn keyed_on(plane: Option<&Path>) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let bytes = plane.map_or(b"no plane".as_slice(), |plane| {
        plane.as_os_str().as_encoded_bytes()
    });
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl Hooks {
    /// Nothing listening: every chat is `unknown`, which is what the spec says a harness with
    /// no state hook shows. The app runs perfectly well like this.
    pub fn deaf(plane: PlaneId) -> Self {
        Self {
            plane,
            board: Arc::new(Mutex::new(Board::new())),
            reading: Mutex::new(None),
            socket: None,
            tokens: None,
            answering: Arc::new(Mutex::new(None)),
            heard: Arc::new(Mutex::new(None)),
            following: Arc::new(Mutex::new(None)),
            all_reports: Arc::new(Mutex::new(None)),
            waits: Arc::new(Mutex::new(None)),
            saved: Arc::new(Mutex::new(None)),
            events: Arc::new(Mutex::new(None)),
            asks: Arc::new(HookAsks::new(Arc::new(Asks::new()))),
            asks_told: Arc::new(Mutex::new(None)),
            touching: Arc::new(Mutex::new(None)),
            blocked: Arc::new(Mutex::new(None)),
            secret_exec: Arc::new(Mutex::new(None)),
        }
    }

    /// Listens on `socket`, handing each change to `moved`.
    ///
    /// A socket that cannot be opened is not worth refusing to start over: the app comes up
    /// with every chat `unknown` and says so on stderr, which is a working app with one
    /// feature missing rather than no app at all.
    pub fn listening_on(
        plane: PlaneId,
        at: &Where,
        moved: Teller,
        told_by_hand: ByHandTeller,
    ) -> std::io::Result<Self> {
        let listener = Listener::bind(&at.within, &at.socket)?;
        let socket = listener.path().to_path_buf();
        let tokens = listener.tokens();
        let board = Arc::new(Mutex::new(Board::new()));
        let answering: Arc<Mutex<Option<Answering>>> = Arc::new(Mutex::new(None));
        let heard: Arc<Mutex<Option<Heard>>> = Arc::new(Mutex::new(None));
        let following: Arc<Mutex<Option<Following>>> = Arc::new(Mutex::new(None));
        let all_reports: Arc<Mutex<Option<Heard>>> = Arc::new(Mutex::new(None));
        let waits: Arc<Mutex<Option<WaitsOf>>> = Arc::new(Mutex::new(None));
        let saved: Arc<Mutex<Option<SavedHeard>>> = Arc::new(Mutex::new(None));
        let events: Arc<Mutex<Option<Events>>> = Arc::new(Mutex::new(None));
        let asks = Arc::new(HookAsks::new(Arc::new(Asks::new())));
        let asks_told: crate::asking::Telling = Arc::new(Mutex::new(None));
        let touching: Arc<Mutex<Option<Touches>>> = Arc::new(Mutex::new(None));
        let blocked: Arc<Mutex<Option<Blocks>>> = Arc::new(Mutex::new(None));
        let secret_exec: Arc<Mutex<Option<SecretExecs>>> = Arc::new(Mutex::new(None));
        let reading = listener.hear(Hearing {
            // A brokered `secret exec` (#1407), held on its connection while the command runs.
            // Taken out of the lock before it runs, as an answer is.
            secret_exec: {
                let secret_exec = Arc::clone(&secret_exec);
                Box::new(move |ask, reader, writer| {
                    let runs = secret_exec
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    match runs {
                        Some(runs) => runs(ask, reader, writer),
                        None => purlis_core::secrets::brokered::not_answered(writer),
                    }
                })
            },
            // A sandbox block (#1338): kept for `purlis doctor`'s count, then handed on for the
            // chat's Notice. It holds an operation and a kind only, so keeping it keeps nothing
            // of what the chat ran. Taken out of the lock before it runs, as an answer is.
            blocked: {
                let blocked = Arc::clone(&blocked);
                let plane = plane.clone();
                let throttle = Mutex::new(purlis_core::sandboxblock::Throttle::default());
                Box::new(move |block| {
                    let at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.as_secs());
                    heard_block(
                        plane.root(),
                        &throttle,
                        &blocked,
                        block,
                        (at, std::time::Instant::now()),
                    );
                })
            },
            permission: crate::asking::permitting(
                plane.clone(),
                Arc::clone(&asks),
                Arc::clone(&asks_told),
            ),
            // A file a chat's tool touched (FM-6): handed on, never recorded (D-86a). Taken out
            // of the lock before it runs, as an answer is.
            touching: {
                let touching = Arc::clone(&touching);
                Box::new(move |touched| {
                    let listener = touching
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    if let Some(listener) = listener {
                        listener(touched);
                    }
                })
            },
            each: {
                let board = Arc::clone(&board);
                let plane = plane.clone();
                let heard = Arc::clone(&heard);
                let following = Arc::clone(&following);
                let all_reports = Arc::clone(&all_reports);
                let waits = Arc::clone(&waits);
                let moved = Arc::clone(&moved);
                let events = Arc::clone(&events);
                Box::new(move |report| {
                    let applied = apply(&board, &plane, &report, waits_at(&waits, &report));
                    let followed = applied.followed();
                    // The run a `/clear` begins is the host's, minted here, so the record
                    // names it whether or not this machine keeps an event log (ADR 0066).
                    let begun = (followed == purlis_core::eventlog::Followed::Moved)
                        .then(purlis_core::reopen::mint);
                    let recorded = record(&events, |log| {
                        log.report_with(plane.root(), &report, followed, begun.as_deref())
                    });
                    // Before the window is told, so the record already names the conversation
                    // by the time anything the move prompts could ask for it. Whether or not a
                    // reader sees a difference: after `/clear` the chat may be in the state it
                    // was in, under a conversation it was not.
                    if let Some(id) = applied.followed {
                        let listener = following
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone();
                        if let Some(listener) = listener {
                            listener(report.chat, &id, begun.as_deref());
                        }
                    }
                    if let Some(what) = applied.moved {
                        moved(what);
                        // Only a report that moved the board: one for a chat it does not have,
                        // or one ADR 0024's rules refused, is heard by nobody. A chat's first
                        // start always moves it (`unknown` to `waiting`). Taken out of the
                        // lock before it runs, as an answer is.
                        let listener = heard.lock().unwrap_or_else(PoisonError::into_inner).clone();
                        if let Some(listener) = listener {
                            listener(&report);
                        }
                    }
                    let listener = all_reports
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    if let Some(listener) = listener {
                        listener(&report);
                    }
                    recorded
                })
            },
            answer: {
                let answering = Arc::clone(&answering);
                Box::new(move |connection, ask| {
                    // Taken out of the lock before it runs: an open starts a program, and a
                    // program that dies at once reaches back into this plane.
                    let answer = answering
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    match answer {
                        Some(answer) => answer(connection, ask),
                        None => Answer::No {
                            why: NOTHING_ANSWERS.to_owned(),
                        },
                    }
                })
            },
            noticed: {
                let plane = plane.clone();
                Box::new(move |notice| {
                    if let Some(told) = by_hand(&plane, notice) {
                        told_by_hand(told);
                    }
                })
            },
            saved: {
                let saved = Arc::clone(&saved);
                Box::new(move |record| {
                    let listener = saved.lock().unwrap_or_else(PoisonError::into_inner).clone();
                    match listener {
                        Some(listener) => listener(record),
                        None => tracing::warn!(
                            "purlis: chat {} saved a session record before this project could \
                             hear it, so nothing was closed",
                            record.chat
                        ),
                    }
                })
            },
            refused: {
                // A refused commit (SQ-16): an event in the log, and a needs-you item on the
                // chat, built under the same hold as the change, as every other move is. Taken
                // only once the event is durable (FD-30).
                let board = Arc::clone(&board);
                let plane = plane.clone();
                let events = Arc::clone(&events);
                let moved = Arc::clone(&moved);
                Box::new(move |refused| {
                    let recorded = record(&events, |log| log.refused(plane.root(), refused.chat));
                    let what = {
                        moving(&mut held_board(&board), &plane, refused.chat, |board| {
                            board.commit_refused(refused.chat, &refused.commit_refused)
                        })
                    };
                    if let Some(what) = what {
                        moved(what);
                    }
                    recorded
                })
            },
            tool: {
                let events = Arc::clone(&events);
                let board = Arc::clone(&board);
                let plane = plane.clone();
                Box::new(move |call| {
                    let recorded = record(&events, |log| {
                        log.tool(plane.root(), &call, std::time::Instant::now())
                    });
                    // A child agent's tool call shows it under its chat: a Codex child's
                    // first hook is one (FD-18). Built under the same hold as the change.
                    if let Some(agent) = &call.agent {
                        let what = {
                            let mut guard = held_board(&board);
                            guard
                                .child_heard(call.chat, agent)
                                .then(|| seen_by(&guard, &plane, call.chat))
                        };
                        if let Some(what) = what {
                            moved(what);
                        }
                    }
                    recorded
                })
            },
        });
        Ok(Self {
            plane,
            board,
            reading: Mutex::new(Some(reading)),
            socket: Some(socket),
            tokens: Some(tokens),
            answering,
            heard,
            following,
            all_reports,
            waits,
            saved,
            events,
            asks,
            asks_told,
            touching,
            blocked,
            secret_exec,
        })
    }

    /// The permission asks this project's chats hold open (HP-6).
    pub fn asks(&self) -> &HookAsks {
        &self.asks
    }

    /// Tells `teller` this project's asks each time they change (HP-6).
    pub fn tell_asks_to(&self, teller: crate::asking::Teller) {
        *self
            .asks_told
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(teller);
    }

    /// Answers chat `session`'s ask `ask` with `option`, as the operator in the window, on the
    /// hook that waits on it; then tells the window the asks as they now are, answered or not.
    pub fn answer(&self, session: u32, ask: &str, option: &str) -> Result<(), String> {
        let id: purlis_core::harness::asks::AskId = ask.parse()?;
        let answered = self.asks.answer(
            &session.to_string(),
            &id,
            option,
            crate::asking::the_window(),
            std::time::Instant::now(),
        );
        crate::asking::tell(&self.plane, &self.asks, &self.asks_told);
        answered.map(|_| ()).map_err(|refused| refused.to_string())
    }

    /// Records every hook call this project's channel hears into `events` from now on (FD-9).
    pub fn record_into(&self, events: Events) {
        *self.events.lock().unwrap_or_else(PoisonError::into_inner) = Some(events);
    }

    /// **Audits a sandbox grant or revoke** (#1342) in the host's event log, under chat
    /// `number` where it came from one, and made durable before it answers. Refused where this
    /// machine has no event log open: a grant nobody recorded is never made.
    pub fn record_grant(
        &self,
        root: &Path,
        number: Option<u32>,
        audited: &purlis_core::sandbox::grant::Audited<'_>,
    ) -> Result<(), String> {
        let Some(events) = self
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        else {
            return Err(
                "purlis's event log is not open on this machine, so nothing was changed: every \
                 sandbox grant is recorded"
                    .to_owned(),
            );
        };
        let mut log = events.lock().unwrap_or_else(PoisonError::into_inner);
        log.sandbox_grant(root, number, audited)
            .map(|_| ())
            .map_err(|why| format!("the event log refused it ({why}), so nothing was changed"))
    }

    /// **Audits a dispatch grant or revoke** (#1437) in the host's event log, under chat
    /// `number` where it was allowed from one, and made durable before it answers. Refused
    /// where this machine has no event log open: a grant nobody recorded is never made.
    pub fn record_dispatch_grant(
        &self,
        root: &Path,
        number: Option<u32>,
        audited: &purlis_core::dispatchgrant::Audited<'_>,
    ) -> Result<(), String> {
        let Some(events) = self
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        else {
            return Err(
                "purlis's event log is not open on this machine, so nothing was changed: every \
                 dispatch grant is recorded"
                    .to_owned(),
            );
        };
        let mut log = events.lock().unwrap_or_else(PoisonError::into_inner);
        log.dispatch_grant(root, number, audited)
            .map(|_| ())
            .map_err(|why| format!("the event log refused it ({why}), so nothing was changed"))
    }

    /// Drains this project's hook spool into the event log (FD-30, ADR 0068 §6): the lines its
    /// chats' hooks spooled while no host took them, each checked, and every gap and rejected
    /// line recorded as such.
    ///
    /// **Before any chat starts**, as a project is opened: a chat the reopen record names is
    /// told to the log first, so a line it spooled is recorded under the run it ran in. With
    /// no event log, or no channel, nothing is drained and the spool waits for a host that has
    /// both.
    ///
    /// **A chat the reopen record does not bring back has ended** (V99i): once the drain has
    /// finished, its spool key is dropped. The chats it does bring back keep theirs, with what
    /// was drained under each, so a hook of theirs that is still about loses no line.
    pub fn drain_spool(&self) {
        let Some(socket) = &self.socket else { return };
        let Some(events) = self
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        else {
            return;
        };
        let root = self.plane.root();
        let mut log = events.lock().unwrap_or_else(PoisonError::into_inner);
        // The numbers of the chats this open brings back.
        let mut back = Vec::new();
        if purlis_core::reopen::path(root).is_file()
            && let Ok(record) = purlis_core::reopen::read_or_refusal(root)
        {
            for chat in record.chats {
                back.extend(chat.number);
                if let (Some(number), Some(id), Some(run)) =
                    (chat.number, chat.identity.id, chat.identity.run)
                {
                    log.knows(
                        root,
                        number,
                        purlis_core::eventlog::RunOf {
                            chat: &id,
                            run: &run,
                        },
                    );
                }
            }
        }
        let durable = log.durable();
        let drained = purlis_core::hookwire::spool::drain_at_open(
            &purlis_core::hookwire::spool::dir_for(socket),
            &back,
            &mut |item| {
                let event = log.spooled(root, item)?;
                durable.through(event.seq)
            },
        );
        if let Err(why) = drained {
            tracing::warn!(
                "purlis: the hook spool was not drained ({why}); it is drained at the next start"
            );
        }
    }

    /// Chat `chat` has ended, by its close or by another start taking its place: what its hooks
    /// spooled while this host was busy is drained into the event log, and then its spool keys
    /// are dropped, so no key outlives its chat (V99i, ADR 0068 §6).
    ///
    /// With no event log there is nowhere to record a line, and the keys are dropped all the
    /// same. A drain the log refuses leaves the keys and the lines for the next open of the
    /// project.
    ///
    /// **The event log is held for one item at a time**, as a live line holds it, and not at
    /// all for a chat that spooled nothing: every other chat's hooks go on being recorded
    /// while this one's spool is drained, where a hold across the drain would have each of
    /// them wait and, past 250 ms, spool.
    pub fn chat_ended(&self, chat: u32) {
        use purlis_core::hookwire::spool;
        let Some(socket) = &self.socket else { return };
        let dir = spool::dir_for(socket);
        let events = self
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let ended = match events {
            Some(events) => {
                let root = self.plane.root();
                spool::end_chat(&dir, chat, &mut |item| {
                    let (event, durable) = {
                        let mut log = events.lock().unwrap_or_else(PoisonError::into_inner);
                        (log.spooled(root, item), log.durable())
                    };
                    durable.through(event?.seq)
                })
            }
            None => spool::forget_chat(&dir, chat),
        };
        if let Err(why) = ended {
            tracing::warn!(
                "purlis: chat {chat}'s hook spool was not drained as it ended ({why}); its \
                 key is kept until the next open of the project"
            );
        }
    }

    /// Stops listening and releases the socket. The plane on disk is untouched.
    ///
    /// Dropping the reader is what unlinks the socket file and joins the thread, and this is
    /// where a closed plane does it — so a plane that is opened again binds a socket of its
    /// own rather than inheriting a live one's path.
    pub fn stop(&self) {
        drop(
            self.reading
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
    }

    /// Whether this is still listening. Only a test asks.
    #[cfg(test)]
    pub fn listening(&self) -> bool {
        self.reading
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// Who answers asks on this socket from now on.
    pub fn answer_with(&self, answering: Answering) {
        *self
            .answering
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(answering);
    }

    /// Who is told each report the board takes from now on.
    pub fn when_heard(&self, heard: Heard) {
        *self.heard.lock().unwrap_or_else(PoisonError::into_inner) = Some(heard);
    }

    /// Who says, from now on, what a chat waits on as the end of its turn is applied (#1491).
    pub fn waits_by(&self, waits: WaitsOf) {
        *self.waits.lock().unwrap_or_else(PoisonError::into_inner) = Some(waits);
    }

    /// Applies `report` as the socket's listener does, with what its chat waits on read first,
    /// and answers what the window must now be told. For a test that stands in for a chat's
    /// hook without a socket: the listener's own path, less the event log and who listens.
    #[cfg(test)]
    pub(crate) fn hear(&self, report: &Report) -> Option<Moved> {
        apply(
            &self.board,
            &self.plane,
            report,
            waits_at(&self.waits, report),
        )
        .moved
    }

    /// Who is told every report on this socket from now on, after the board has had it, whether
    /// or not it moved anything.
    pub fn when_reported(&self, heard: Heard) {
        *self
            .all_reports
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(heard);
    }

    /// Who is told, from now on, each path a chat's file tool touched (FM-6), as the chat said
    /// it: the plane confines it to the chat's folder before the window hears of it.
    pub fn when_touching(&self, touches: Touches) {
        *self.touching.lock().unwrap_or_else(PoisonError::into_inner) = Some(touches);
    }

    /// Who is told, from now on, each sandbox block a chat's hook found (#1338), once it is kept.
    pub fn when_blocked(&self, blocks: Blocks) {
        *self.blocked.lock().unwrap_or_else(PoisonError::into_inner) = Some(blocks);
    }

    /// Who runs, from now on, each brokered `secret exec` a chat asks for (#1407).
    pub fn exec_secrets_with(&self, runs: SecretExecs) {
        *self
            .secret_exec
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(runs);
    }

    /// Who is told, from now on, each session record a chat says it saved (ADR 0064).
    pub fn when_saved(&self, saved: SavedHeard) {
        *self.saved.lock().unwrap_or_else(PoisonError::into_inner) = Some(saved);
    }

    /// Who is told, from now on, each time a chat's own harness moves it onto another
    /// conversation.
    ///
    /// **Only what the board took.** It is told the conversation [`Board::conversation`]
    /// answers after a report, and only when that changed — so a report ADR 0024's rules
    /// refused (a nested harness's, [`Conversation::Contradicted`], a
    /// [`Conversation::Foreign`] after adoption) is told to nobody, because it moved nothing.
    ///
    /// [`Conversation::Contradicted`]: purlis_core::hookwire::Conversation::Contradicted
    /// [`Conversation::Foreign`]: purlis_core::hookwire::Conversation::Foreign
    pub fn when_it_follows(&self, following: Following) {
        *self
            .following
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(following);
    }

    /// Where this listens. Only a test asks: a chat is told through [`Hooks::reporting`].
    #[cfg(test)]
    pub fn socket(&self) -> Option<&Path> {
        self.socket.as_deref()
    }

    /// Whether the asker on `connection` hung up while its ask was being answered
    /// (`ChatTokens::asker_gone`): what ends a wait nobody is waiting for any more (#1441).
    pub fn asker_gone(&self, connection: u64) -> bool {
        self.tokens
            .as_ref()
            .is_some_and(|tokens| tokens.asker_gone(connection))
    }

    /// What a chat started in this plane is told to report to: the socket and the tokens it
    /// checks, or nothing when this is not listening.
    pub fn reporting(&self) -> Option<crate::sessions::Reporting> {
        Some(crate::sessions::Reporting {
            socket: self.socket.clone()?,
            tokens: Arc::clone(self.tokens.as_ref()?),
        })
    }

    /// A fresh token for chat `chat`, as the app issues one when the chat starts: what a test
    /// standing in for that chat's hook sends with.
    #[cfg(test)]
    pub fn token_for(&self, chat: u32) -> purlis_core::hookwire::ChatToken {
        self.tokens
            .as_ref()
            .expect("the plane is listening")
            .issue_to_this_process(chat)
            .expect("a token")
    }

    pub fn board(&self) -> MutexGuard<'_, Board> {
        held_board(&self.board)
    }

    /// The board itself, for a caller that has to outlive this handle — the exit reporting,
    /// which is armed before the app manages anything.
    pub fn shared_board(&self) -> Arc<Mutex<Board>> {
        Arc::clone(&self.board)
    }
}

/// The in-process host's board. Each call is documented once, on [`ChatBoard`].
impl ChatBoard for Hooks {
    fn glance(&self, session: u32) -> Glance {
        let board = self.board();
        Glance {
            state: board.state(session),
            asking: board.asking(session),
            turns: board.turns(session),
        }
    }

    fn conversation(&self, session: u32) -> Option<String> {
        self.board().conversation(session).map(str::to_owned)
    }

    fn now(&self, session: u32) -> Moved {
        now(&self.board, self.plane.clone(), session)
    }

    fn closed(&self, session: u32) -> Moved {
        let mut board = self.board();
        board.closed(session);
        seen_by(&board, &self.plane, session)
    }

    fn ignored(&self, session: u32) -> Moved {
        let mut board = self.board();
        board.ignored(session);
        seen_by(&board, &self.plane, session)
    }

    fn reported_back(&self, session: u32, from: &str) -> Option<Moved> {
        let mut board = self.board();
        board
            .reported_back(session, from)
            .then(|| seen_by(&board, &self.plane, session))
    }

    fn stopped_below(&self, session: u32, from: &str) -> Option<Moved> {
        let mut board = self.board();
        board
            .stopped_below(session, from)
            .then(|| seen_by(&board, &self.plane, session))
    }

    fn needs(&self, session: u32, need: purlis_core::state::Need) -> Option<Moved> {
        moving(&mut self.board(), &self.plane, session, |board| {
            board.needs(session, need)
        })
    }

    fn reported_to_its_asker(&self, session: u32) {
        self.board().reported_to_its_asker(session);
    }

    fn rested(&self, session: u32) -> Option<Moved> {
        moving(&mut self.board(), &self.plane, session, |board| {
            board.rested(session)
        })
    }

    fn task_failed(&self, session: u32, failed: purlis_core::state::FailedTask) -> Option<Moved> {
        moving(&mut self.board(), &self.plane, session, |board| {
            board.task_failed(session, failed)
        })
    }

    fn failure_cleared(&self, session: u32, id: &str) -> Option<Moved> {
        moving(&mut self.board(), &self.plane, session, |board| {
            board.failure_cleared(session, id)
        })
    }
}

/// Makes one move of chat `session` on the board, and answers what the window must now be
/// told where a reader would see a difference: [`seen_by`], with whether the move raised
/// something for the person (`Moved::raised`), read off where the chat stood before and
/// after. One hold from the read before to the snapshot, as every move's is.
fn moving(
    board: &mut Board,
    plane: &PlaneId,
    session: u32,
    act: impl FnOnce(&mut Board) -> bool,
) -> Option<Moved> {
    let was = board.standing(session);
    if !act(board) {
        return None;
    }
    let mut moved = seen_by(board, plane, session);
    moved.raised = purlis_core::state::raised(was, board.standing(session));
    Some(moved)
}

/// What the chat `report` names waits on, asked only of a report that can end its turn or
/// nudge it after one: a `Stop` or a `Notification` of the chat's own. Every other report
/// raises no needs-you item this could hold back, so the project's records are not read for
/// it. **Asked before the board is taken**, never under it: the answer reads the chats, the
/// board and the tasks' ledger in turn.
fn waits_at(waits: &Mutex<Option<WaitsOf>>, report: &Report) -> purlis_core::state::Waits {
    use purlis_core::state::Event;
    if report.agent.is_some() || !matches!(report.event, Event::Stop | Event::Notification) {
        return purlis_core::state::Waits::default();
    }
    let asked = waits.lock().unwrap_or_else(PoisonError::into_inner).clone();
    asked.map_or_else(purlis_core::state::Waits::default, |asked| {
        asked(report.chat, report.event == Event::Stop)
    })
}

/// What a reader sees for this chat right now.
pub fn now(board: &Mutex<Board>, plane: PlaneId, session: u32) -> Moved {
    seen_by(&held_board(board), &plane, session)
}

/// The same, for a caller that is already holding the board.
///
/// **Only ever called with the board held**, which is what makes [`sequence`] an order over
/// the board's states: the number is taken inside the same hold as the read.
fn seen_by(board: &Board, plane: &PlaneId, session: u32) -> Moved {
    let queue = board.needs_you();
    Moved {
        sequence: sequence(),
        plane: plane.clone(),
        session,
        state: word(board.state(session)),
        needs_you: queue.contains(&session),
        queue,
        moved_at: board.moved_at(session),
        reports: board.reports(session),
        refusals: board.refusals(session),
        stopped: Some(board.stopped_of(session)).filter(|stopped| !stopped.is_empty()),
        // What the app found, then the tasks of its that came to nothing (#1491): the item
        // says the latest, and a failure is the one the person has not been told of.
        needs: Some(
            board
                .needs_of(session)
                .into_iter()
                .map(Need::from)
                .chain(board.failed_tasks(session).into_iter().map(Need::from))
                .collect::<Vec<Need>>(),
        )
        .filter(|needs| !needs.is_empty()),
        counts_only: false,
        raised: false,
        children: board
            .children(session)
            .into_iter()
            .map(|child| ChildAgent {
                agent: child.agent,
                state: word(child.state),
            })
            .collect(),
    }
}

/// The next snapshot's number. See [`Moved::sequence`].
///
/// **The process's and not one board's**, which is stronger than the window needs and costs
/// nothing: a project closed and opened again in one run gets a new board, and a count that
/// began again at one would have a window still holding the old board's numbers drop every
/// snapshot of the new one. Taken while a board is held, so for any one board the numbers
/// run in the order its states were read.
///
/// Saturating, and a `u32` for the reason [`Board::moved_at`] gives (`specta` cannot carry a
/// `u64`). At four billion snapshots every later one is numbered the same, and the window
/// keeps a snapshot numbered the same as the one it holds — so the app falls back to taking
/// events in the order they land, which is what it did before there was a number, rather than
/// stopping listening.
fn sequence() -> u32 {
    static TAKEN: AtomicU32 = AtomicU32::new(0);
    let was = TAKEN
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            Some(n.saturating_add(1))
        })
        .unwrap_or_else(|n| n);
    was.saturating_add(1)
}

/// What one report did.
struct Applied {
    /// What a reader would now see differently, or nothing when no reader would.
    moved: Option<Moved>,
    /// The conversation the chat is in now, where the report moved it onto one: the first id
    /// a Codex or opencode chat names, or the one a Claude Code chat's own process moved to on
    /// `/clear` (C6). Nothing for a report that left the chat where it was.
    followed: Option<String>,
    /// The conversation the chat was in before the report, where it was in one: with
    /// `followed`, what tells a harness naming its conversation from `/clear` (ADR 0066).
    was: Option<String>,
}

impl Applied {
    /// What the board did with the chat's conversation, as the event log reads a new run.
    fn followed(&self) -> purlis_core::eventlog::Followed {
        use purlis_core::eventlog::Followed;
        match (&self.followed, &self.was) {
            (None, _) => Followed::No,
            (Some(_), None) => Followed::FirstNamed,
            (Some(_), Some(_)) => Followed::Moved,
        }
    }
}

/// Applies one report, answering with what a reader would now see differently and which
/// conversation, if a new one, the chat is now in.
///
/// The answer is built under the SAME hold as the change. Reports arrive on a thread each, so
/// dropping the lock in between let two of them interleave — mutate, mutate, read, read — and
/// the window could then be sent the older of the two snapshots last and keep it until the
/// next event. A review found it.
///
/// **Which conversation is the board's answer, read before and after**, and not the report's:
/// [`Board::reported`] is the one place that decides whether a report is the chat's own
/// harness speaking (ADR 0024), and a second reading of the report here would be a second
/// answer to that question.
///
/// `waits` is what the chat waits on that is not the person ([`waits_at`], #1491), read
/// before the board was taken.
fn apply(
    board: &Mutex<Board>,
    plane: &PlaneId,
    report: &Report,
    waits: purlis_core::state::Waits,
) -> Applied {
    let mut guard = held_board(board);
    let was = guard.conversation(report.chat).map(str::to_owned);
    let moved = moving(&mut guard, plane, report.chat, |board| {
        board.reported_while(report, waits)
    });
    let now = guard.conversation(report.chat);
    let followed = now
        .filter(|now| was.as_deref() != Some(*now))
        .map(str::to_owned);
    Applied {
        moved,
        followed,
        was,
    }
}

/// Writes one event into the host's log, when there is one, and answers once it is durable:
/// the hook is told its line is taken only on `Ok`, and answers its harness after that (ADR
/// 0075 §7, FD-30). The `fsync` is outside the log's lock, so hooks recorded at once share one.
///
/// A write or a sync that fails is said in the app's log and is the error: the hook is not
/// told its line was taken, so it spools the line and the next open records it. With no event
/// log there is nothing to wait for, and that is `Ok`.
fn record(
    events: &Mutex<Option<Events>>,
    write: impl FnOnce(
        &mut purlis_core::eventlog::Recorder,
    ) -> std::io::Result<purlis_core::eventlog::Event>,
) -> std::io::Result<()> {
    let Some(held) = events
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
    else {
        return Ok(());
    };
    let (event, durable) = {
        let mut log = held.lock().unwrap_or_else(PoisonError::into_inner);
        (write(&mut log), log.durable())
    };
    event
        .and_then(|event| durable.through(event.seq))
        .inspect_err(|why| {
            tracing::warn!("purlis: a hook call was not recorded durably ({why})");
        })
}

/// The board, whether or not a thread panicked while holding it.
///
/// A poisoned board is one whose last change may not have finished; the state it holds is
/// still the best answer there is, and refusing to draw anything at all would be worse.
pub fn held_board(board: &Mutex<Board>) -> MutexGuard<'_, Board> {
    board
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The word the window draws, which is the word the spec uses.
pub fn word(state: State) -> String {
    match state {
        State::Unknown => "unknown",
        State::Running => "running",
        State::Waiting => "waiting",
        State::Done => "done",
        State::Failed => "failed",
    }
    .to_owned()
}

/// What an exit says about a chat: the code, or none for a program killed by a signal.
pub fn code_of(exit: &Exit) -> Option<i32> {
    match exit {
        Exit::Code(code) => i32::try_from(*code).ok(),
        // A signal leaves no code behind, and it is not a clean end.
        Exit::Signal(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A board holding chat `session`, which has stopped and is asking for you.
    fn asking(session: u32) -> Hooks {
        // Spelled the way a window hands one back; only the registry mints one for real.
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks = Hooks::deaf(plane);
        hooks.board().opened(session, None, None);
        assert!(
            apply(
                &hooks.board,
                &hooks.plane,
                &stop(session),
                purlis_core::state::Waits::default()
            )
            .moved
            .is_some()
        );
        hooks
    }

    fn stop(session: u32) -> Report {
        Report {
            chat: session,
            event: purlis_core::state::Event::Stop,
            conversation: purlis_core::hookwire::Conversation::Unknown,
            pid: None,
            agent: None,
            detail: purlis_core::state::Detail::default(),
        }
    }

    #[test]
    fn a_report_taken_before_a_close_is_numbered_before_it() {
        // charter-app#248, the race #256 left open: a report's `Moved` is sent on the thread
        // that read it, after the board is let go, so it can reach the window AFTER a close
        // that came later. The number is what lets the window tell which is newer.
        let hooks = asking(7);
        let late = apply(
            &hooks.board,
            &hooks.plane,
            &stop(7),
            purlis_core::state::Waits::default(),
        )
        .moved;
        hooks.board().ignored(7);
        let report = apply(
            &hooks.board,
            &hooks.plane,
            &stop(7),
            purlis_core::state::Waits::default(),
        )
        .moved
        .expect("a new request");

        let close = hooks.closed(7);

        assert!(
            late.is_none(),
            "a stop on a chat already asking moved nothing"
        );
        assert_eq!(report.queue, vec![7]);
        assert!(close.queue.is_empty());
        assert!(
            report.sequence < close.sequence,
            "the close ({}) is not numbered after the report it follows ({})",
            close.sequence,
            report.sequence
        );
    }

    #[test]
    fn every_snapshot_is_numbered_after_the_one_before_it() {
        let hooks = asking(7);

        let first = hooks.now(7);
        let second = hooks.now(7);

        assert!(first.sequence < second.sequence);
    }

    #[test]
    fn ignoring_a_chat_tells_a_queue_without_it_and_the_chat_still_waiting() {
        let hooks = asking(7);
        let asked = hooks.now(7);

        let ignored = hooks.ignored(7);

        assert!(ignored.queue.is_empty());
        assert!(!ignored.needs_you);
        assert_eq!(ignored.state, "waiting");
        assert!(ignored.sequence > asked.sequence);
    }

    #[test]
    fn the_socket_sits_beside_the_record_the_app_already_writes() {
        // `.charter/app/` is where M1.7 put `reopen.json`. One place for what the app keeps.
        assert_eq!(
            socket_for(Some(Path::new("/Users/o/plane"))).socket,
            PathBuf::from("/Users/o/plane/.charter/app/hooks.sock")
        );
    }

    #[test]
    fn a_run_outside_any_plane_still_has_somewhere_to_listen() {
        // The channel is the APP's, not the plane's. A chat started outside a plane is still
        // a chat, and answering `unknown` for it because there is no `charter.toml` above it
        // would be a strange rule — it is also how this was found: it made every chat in the
        // scenario tests unknown.
        let socket = socket_for(None).socket;

        assert!(socket.starts_with(private_dir()));
        assert!(socket.as_os_str().len() <= LONGEST_SOCKET_PATH);
    }

    #[test]
    fn a_plane_too_deep_for_a_unix_socket_path_falls_back_instead_of_failing() {
        // macOS allows 104 bytes for the whole path. A plane checked out under a long CI
        // path, or a deeply nested workspace, would otherwise get no event channel at all —
        // and the failure would look like hooks being broken rather than a path being long.
        let deep = PathBuf::from("/Users/operator").join("a".repeat(120));

        let socket = socket_for(Some(&deep)).socket;

        assert!(
            socket.as_os_str().len() <= LONGEST_SOCKET_PATH,
            "{} is {} bytes",
            socket.display(),
            socket.as_os_str().len()
        );
        assert!(socket.starts_with(private_dir()));
    }

    #[test]
    fn the_fallback_prefers_the_runtime_directory_where_the_platform_has_one() {
        // On Linux the temp directory is `/tmp`, which everyone can write to; the standard
        // per-user place for a socket is `$XDG_RUNTIME_DIR`. macOS names no such variable and
        // its own temp directory is already per-user.
        match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(runtime) => assert!(socket_for(None).socket.starts_with(runtime)),
            None => assert!(socket_for(None).socket.starts_with(std::env::temp_dir())),
        }
    }

    #[test]
    fn two_deep_planes_do_not_share_one_socket() {
        let one = PathBuf::from("/Users/operator").join("a".repeat(120));
        let two = PathBuf::from("/Users/operator").join("b".repeat(120));

        assert_ne!(socket_for(Some(&one)).socket, socket_for(Some(&two)).socket);
    }

    #[test]
    fn a_plane_and_no_plane_do_not_share_one_socket() {
        let deep = PathBuf::from("/Users/operator").join("a".repeat(120));

        assert_ne!(socket_for(Some(&deep)).socket, socket_for(None).socket);
    }

    #[test]
    fn a_harness_started_by_hand_in_a_shell_tab_is_told_to_the_window_with_its_plane() {
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let hooks = Hooks::listening_on(
            plane.clone(),
            &at,
            Arc::new(|_| panic!("a harness started by hand moves no chat")),
            Arc::new(move |told| tx.lock().unwrap().send(told).unwrap()),
        )
        .expect("listening");

        purlis_core::hookwire::tell(
            hooks.socket().expect("a socket"),
            Some(&hooks.token_for(3)),
            &StartedByHand {
                chat: 3,
                started_by_hand: "codex".to_owned(),
                cwd: Some(PathBuf::from("/work/alpha")),
            },
        )
        .expect("told");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(ByHand {
                plane,
                session: 3,
                harness: "codex".to_owned(),
                cwd: Some("/work/alpha".to_owned()),
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_permission_prompt_is_told_to_the_window_and_the_window_s_answer_goes_back_on_its_hook() {
        // HP-6: Claude Code's `PermissionRequest` hook, on a project's own socket, becomes an
        // ask the window is told; the window answers it, and the hook hears the option.
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks = Hooks::listening_on(plane.clone(), &at, Arc::new(|_| {}), Arc::new(|_| {}))
            .expect("listening");
        let (tx, told) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        hooks.tell_asks_to(Arc::new(move |asking| {
            tx.lock().unwrap().send(asking).unwrap()
        }));

        let hook = {
            let socket = hooks.socket().expect("a socket").to_path_buf();
            let token = hooks.token_for(3);
            std::thread::spawn(move || {
                // In the host's own process, which no ancestry check could admit: the
                // exchange alone, as a hook makes it once it has admitted its host.
                purlis_core::hookwire::permission::ask_permission_of_an_admitted_host(
                    std::os::unix::net::UnixStream::connect(&socket).expect("connects"),
                    Some(&token),
                    &purlis_core::hookwire::PermissionAsked {
                        chat: 3,
                        permission_request: purlis_core::harness::hooked::Source::ClaudeCode,
                        payload: serde_json::json!({"tool_name": "Bash",
                            "tool_input": {"command": "npm test"}}),
                    },
                    std::time::Duration::from_secs(30),
                )
            })
        };
        let asking = told
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the window is told");
        assert_eq!(asking.plane, plane);
        let [shown] = asking.asks.as_slice() else {
            panic!("one ask: {asking:?}");
        };
        assert_eq!(shown.session, 3);
        assert!(shown.says.contains("npm test"), "{}", shown.says);
        let labels: Vec<&str> = shown.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, ["Allow", "Deny"]);

        assert_eq!(
            hooks.answer(4, &shown.ask, "allow"),
            Err("no such ask is waiting".to_owned()),
            "another chat's answer never lands on it"
        );
        hooks
            .answer(3, &shown.ask, "allow")
            .expect("the window's answer applies");

        assert_eq!(
            hook.join().expect("the hook").expect("a reply"),
            Some("allow".to_owned())
        );
        assert!(hooks.answer(3, &shown.ask, "allow").is_err(), "once");
    }

    #[test]
    fn every_hook_call_the_channel_hears_is_one_event_in_the_hosts_log() {
        use purlis_core::eventlog::{self, ArgsKey, Log, Recorder};
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let logs = dir.path().join("events");
        hooks.record_into(Arc::new(Mutex::new(Recorder::new(
            Log::open(&logs, "DEVICE").expect("a log"),
            ArgsKey::open(&logs).expect("a key"),
        ))));
        hooks.board().opened(3, Some(Harness::ClaudeCode), None);
        let socket = hooks.socket().expect("a socket");
        let token = hooks.token_for(3);

        purlis_core::hookwire::send(
            socket,
            Some(&token),
            &Report {
                chat: 3,
                event: purlis_core::state::Event::UserPromptSubmit,
                conversation: Default::default(),
                pid: None,
                agent: None,
                detail: Default::default(),
            },
        )
        .expect("sent");
        // Each line is its own connection; the second is sent once the first is in the log.
        let logged = |at_least: usize| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let kinds: Vec<String> = eventlog::read(&logs)
                    .expect("the log reads")
                    .into_iter()
                    .map(|event| event.kind)
                    .collect();
                if kinds.len() >= at_least || std::time::Instant::now() > until {
                    break kinds;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        assert_eq!(logged(2).len(), 2, "the report is in the log");
        purlis_core::hookwire::deliver_tool(
            socket,
            Some(&token),
            &purlis_core::hookwire::ToolCall {
                chat: 3,
                tool_hook: "pretooluse".to_owned(),
                tool: Some("Bash".to_owned()),
                call: Some("toolu_1".to_owned()),
                args: Some(eventlog::args_hash(&serde_json::json!({"command": "ls"}))),
                decision: purlis_core::hookwire::Decision::None,
                rule: None,
                hook_ms: 1,
                agent: None,
                at_ms: 0,
            },
        )
        .expect("told");

        let kinds = logged(3);
        assert_eq!(
            kinds,
            vec!["run.started", "hook.userpromptsubmit", "hook.pretooluse"],
            "one event per hook call, after the run it is under"
        );
    }

    #[test]
    fn recorded_violation_lines_reach_the_window_as_the_notice_says_them() {
        use purlis_core::hookwire::SandboxBlocked;
        use purlis_core::sandboxblock::{Place, detect};
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        // What Claude Code handed `posttoolusefailure-blocked` for a smart close from a clone.
        let payload = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": "purlis session record --title close"},
            "error": "Exit code 1\n<sandbox_violations>\npurlis(48211) deny(1) \
                      file-write-create /plane/workspaces/alpha/sessions/close.md\n\
                      </sandbox_violations>",
        });
        let place = Place {
            root: Path::new("/plane"),
            chat: Path::new("/plane/workspaces/alpha/repo"),
            cwd: Path::new("/plane/workspaces/alpha/repo"),
            home: None,
        };
        let told: Vec<ChatBlocked> = detect(&payload, &place)
            .into_iter()
            .map(|block| {
                blocked(
                    &plane,
                    &SandboxBlocked {
                        chat: 3,
                        sandbox_blocked: block,
                        harness: Some("claude".to_owned()),
                        target: None,
                    },
                )
            })
            .collect();
        assert_eq!(
            told,
            vec![ChatBlocked {
                plane: plane.clone(),
                session: 3,
                operation: "write".to_owned(),
                kind: "project-files".to_owned(),
                ours: true,
                harness: Some("claude".to_owned()),
                said: "a write to the project's own files".to_owned(),
                // purlis's own: a bug to report, never something to allow.
                offer: BlockOffer::None,
                target: None,
                route: None,
                levels: Vec::new(),
            }]
        );
        // A harness word purlis does not start is no harness.
        let other = blocked(
            &plane,
            &SandboxBlocked {
                chat: 3,
                sandbox_blocked: told_block(),
                harness: Some("/home/dev/bin/thing".to_owned()),
                target: None,
            },
        );
        assert_eq!(other.harness, None);
    }

    #[test]
    fn a_heard_block_is_kept_then_told_and_a_flood_is_held_back() {
        use purlis_core::hookwire::SandboxBlocked;
        let dir = tempfile::tempdir().expect("a directory");
        let throttle = Mutex::new(purlis_core::sandboxblock::Throttle::default());
        let told: Arc<Mutex<Vec<SandboxBlocked>>> = Arc::new(Mutex::new(Vec::new()));
        let slot: Mutex<Option<Blocks>> = Mutex::new(Some({
            let told = Arc::clone(&told);
            let root = dir.path().to_path_buf();
            Arc::new(move |block| {
                // Kept before it is told: the doctor's count already has it.
                assert_eq!(
                    purlis_core::sandboxblock::counts(&root, 100)
                        .iter()
                        .map(|count| count.blocks)
                        .sum::<u64>(),
                    told.lock().unwrap().len() as u64 + 1
                );
                told.lock().unwrap().push(block);
            })
        }));
        let now = std::time::Instant::now();
        let line = SandboxBlocked {
            chat: 3,
            sandbox_blocked: told_block(),
            harness: None,
            target: None,
        };
        for _ in 0..50 {
            heard_block(dir.path(), &throttle, &slot, line.clone(), (100, now));
        }
        assert_eq!(
            *told.lock().unwrap(),
            vec![line.clone()],
            "one, however often it is sent"
        );
        assert_eq!(
            purlis_core::sandboxblock::counts(dir.path(), 100)[0].blocks,
            1
        );
    }

    #[test]
    fn a_block_s_notice_offers_allow_only_for_a_host_or_a_folder_and_the_way_out_otherwise() {
        use purlis_core::sandboxblock::{Block, Kind, Operation};
        let project = tempfile::tempdir().expect("a project");
        let root = project.path().canonicalize().expect("real");
        let plane: PlaneId =
            serde_json::from_value(serde_json::json!(root.display().to_string())).expect("an id");
        let told = |operation, kind, ours, target: Option<&str>| {
            let said = blocked(
                &plane,
                &purlis_core::hookwire::SandboxBlocked {
                    chat: 3,
                    sandbox_blocked: Block {
                        operation,
                        kind,
                        ours,
                    },
                    harness: None,
                    target: target.map(str::to_owned),
                },
            );
            (said.offer, said.target, said.route.is_some())
        };
        let host = |target| told(Operation::Connect, Kind::Host, false, target);
        // A host, named where the report named one, typed by the person where it did not.
        assert_eq!(
            host(Some("API.example.com:443")),
            (
                BlockOffer::Host,
                Some("api.example.com:443".to_owned()),
                false
            )
        );
        assert_eq!(host(None), (BlockOffer::Host, None, false));
        // Never proposed: a host the sandbox never lets a chat reach, and a wildcard a line
        // named (a real refusal names one host).
        for target in ["169.254.169.254", "*.amazonaws.com"] {
            assert_eq!(
                host(Some(target)),
                (BlockOffer::Host, None, false),
                "{target}"
            );
        }
        // A write in the project, on the folder it was refused in. A fixture made in a
        // harness's own temp root (an agent's `/private/tmp/claude-<uid>`) is refused with it:
        // that refusal is never lifted for a project that lives there.
        let other = root.join("workspaces/beta/out");
        let in_a_harness_temp_root = root
            .strip_prefix("/private/tmp")
            .ok()
            .and_then(|below| below.components().next())
            .is_some_and(|first| first.as_os_str().to_string_lossy().starts_with("claude-"));
        let (offer, target, why) = told(
            Operation::Write,
            Kind::ProjectFiles,
            false,
            Some(&other.join("x.lock").display().to_string()),
        );
        assert_eq!(target, Some(other.display().to_string()));
        if in_a_harness_temp_root {
            assert_eq!((offer, why), (BlockOffer::Unsandboxed, true));
        } else {
            assert_eq!((offer, why), (BlockOffer::Write, false));
        }
        // Off the allowlist (D-1342-10): no Allow, and the chat without the sandbox instead.
        let home = std::env::var("HOME").expect("a home");
        for path in [
            format!("{home}/Library/LaunchAgents/x.plist"),
            format!("{home}/.config/git/config"),
            "/opt/tool/cache/x.lock".to_owned(),
        ] {
            let (offer, _, why) = told(Operation::Write, Kind::Home, false, Some(&path));
            assert_eq!((offer, why), (BlockOffer::Unsandboxed, true), "{path}");
        }
        // A local socket: the same.
        assert_eq!(
            told(Operation::Connect, Kind::LocalSocket, false, None),
            (BlockOffer::Unsandboxed, None, true)
        );
        // Never granted: the way that works instead.
        for (operation, kind) in [
            (Operation::Write, Kind::ProjectState),
            (Operation::Write, Kind::ProtectedFile),
            (Operation::Read, Kind::Home),
        ] {
            assert_eq!(
                told(operation, kind, false, Some("/x/y")),
                (BlockOffer::Brokered, None, true),
                "{operation:?} {kind:?}"
            );
        }
        // purlis's own operation is reported, never allowed.
        assert_eq!(
            told(Operation::Write, Kind::System, true, Some("/opt/x")),
            (BlockOffer::None, None, false)
        );
    }

    #[test]
    fn a_block_s_notice_offers_nothing_policy_forbids_and_says_who_forbade_it() {
        use crate::sandboxing::GrantLevel;
        use purlis_core::sandbox::policy::Locks;
        let policy = |json: &str| Locks::parse(json, Path::new("/etc/purlis/policy.json"));
        let host = || (BlockOffer::Host, Some("pastebin.example".to_owned()), None);
        let write = || (BlockOffer::Write, Some("/p/out".to_owned()), None);
        let socket = || {
            (
                BlockOffer::Unsandboxed,
                None,
                Some("Not a grant.".to_owned()),
            )
        };
        // No policy: every level, and Start without the sandbox where nothing is granted.
        assert_eq!(
            held(&Locks::none(), host()).3,
            [GrantLevel::Chat, GrantLevel::You, GrantLevel::Project]
        );
        assert_eq!(held(&Locks::none(), socket()).0, BlockOffer::Unsandboxed);
        // Your own hosts forbidden: only everyone in the project.
        let mine = policy(r#"{"owner": "IT", "sandbox": {"personal-hosts": false}}"#);
        assert_eq!(held(&mine, host()).3, [GrantLevel::Project]);
        // A host policy does not allow: no Allow; Start without the sandbox, saying why.
        let listed = policy(r#"{"owner": "IT", "sandbox": {"hosts": ["*.corp.example"]}}"#);
        let (offer, _, route, levels) = held(&listed, host());
        assert_eq!((offer, levels), (BlockOffer::Unsandboxed, Vec::new()));
        let route = route.expect("why");
        assert!(route.contains("not a host policy allows"), "{route}");
        assert!(route.contains("set by IT"), "{route}");
        // Write grants forbidden: no Allow for a folder.
        let writes = policy(r#"{"owner": "IT", "sandbox": {"write-grants": false}}"#);
        assert_eq!(held(&writes, write()).0, BlockOffer::Unsandboxed);
        // …and with the opt-out forbidden too, nothing is offered, and it still says why.
        let both = policy(
            r#"{"owner": "IT", "sandbox": {"write-grants": false, "opt-out": false,
                "hosts": []}}"#,
        );
        for offered in [host(), write(), socket()] {
            let (offer, _, route, levels) = held(&both, offered);
            assert_eq!((offer, levels), (BlockOffer::Policy, Vec::new()));
            let route = route.expect("why");
            assert!(route.contains("Locked by policy, set by IT"), "{route}");
        }
        // What is never granted keeps the way that works, policy or not.
        let brokered = (
            BlockOffer::Brokered,
            None,
            Some("Use the broker.".to_owned()),
        );
        assert_eq!(held(&both, brokered).0, BlockOffer::Brokered);
    }

    fn told_block() -> purlis_core::sandboxblock::Block {
        purlis_core::sandboxblock::Block {
            operation: purlis_core::sandboxblock::Operation::Connect,
            kind: purlis_core::sandboxblock::Kind::Host,
            ours: false,
        }
    }

    #[test]
    fn a_touched_file_reaches_the_window_only_inside_the_chats_folder_and_rated() {
        use purlis_core::hookwire::Touching;
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let gate = Mutex::new(purlis_core::touching::Gate::default());
        let folder = Path::new("/w/branch");
        let now = std::time::Instant::now();
        let said = |path: &str| Touching {
            chat: 3,
            touching: path.to_owned(),
        };

        assert_eq!(
            touched(
                &plane,
                Some(folder),
                &gate,
                &said("/w/branch/src/a.rs"),
                now
            ),
            Some(ChatTouching {
                plane: plane.clone(),
                session: 3,
                path: "src/a.rs".to_owned(),
            })
        );
        for outside in [
            "/w/other/a.rs",
            "/w/branch/../other/a.rs",
            "../x",
            "/etc/hosts",
        ] {
            assert_eq!(
                touched(&plane, Some(folder), &gate, &said(outside), now),
                None,
                "{outside}"
            );
        }
        assert_eq!(
            touched(&plane, None, &gate, &said("/w/branch/a.rs"), now),
            None,
            "a chat with no folder known marks nothing"
        );
        let told = (0..100)
            .filter(|n| {
                touched(&plane, Some(folder), &gate, &said(&format!("f{n}")), now).is_some()
            })
            .count();
        assert!(told < 10, "a flood is cut at the chat's share: {told}");
    }

    #[test]
    fn a_touched_path_reaches_the_window_and_no_file_the_host_writes() {
        use purlis_core::eventlog::{self, ArgsKey, Log, Recorder};
        const CANARY: &str = "CANARY-touched-9e1b";
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let logs = dir.path().join("events");
        hooks.record_into(Arc::new(Mutex::new(Recorder::new(
            Log::open(&logs, "DEVICE").expect("a log"),
            ArgsKey::open(&logs).expect("a key"),
        ))));
        let (tx, heard) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        hooks.when_touching(Arc::new(move |touching| {
            tx.lock().unwrap().send(touching).unwrap();
        }));
        hooks.board().opened(3, Some(Harness::ClaudeCode), None);
        let socket = hooks.socket().expect("a socket");
        let token = hooks.token_for(3);
        let file = format!("/w/branch/{CANARY}.rs");
        let input = serde_json::json!({"file_path": file});

        purlis_core::hookwire::deliver_tool(
            socket,
            Some(&token),
            &purlis_core::hookwire::ToolCall {
                chat: 3,
                tool_hook: "pretooluse-read".to_owned(),
                tool: Some("Read".to_owned()),
                call: Some("toolu_1".to_owned()),
                args: Some(eventlog::args_hash(&input)),
                decision: purlis_core::hookwire::Decision::None,
                rule: None,
                hook_ms: 1,
                agent: None,
                at_ms: 0,
            },
        )
        .expect("told");
        purlis_core::hookwire::touch(
            socket,
            Some(&token),
            &purlis_core::hookwire::Touching {
                chat: 3,
                touching: file.clone(),
            },
        )
        .expect("told");
        let got = heard
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the touch is handed on");
        assert_eq!(got.touching, file);
        drop(hooks);

        let kinds: Vec<String> = eventlog::read(&logs)
            .expect("the log reads")
            .into_iter()
            .map(|event| event.kind)
            .collect();
        assert!(
            kinds.contains(&"hook.pretooluse-read".to_owned()),
            "{kinds:?}"
        );
        let mut stack = vec![dir.path().to_path_buf()];
        while let Some(at) = stack.pop() {
            for entry in std::fs::read_dir(&at).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let text = std::fs::read(&path).unwrap_or_default();
                    assert!(
                        !String::from_utf8_lossy(&text).contains(CANARY),
                        "{} holds the touched path",
                        path.display()
                    );
                }
            }
        }
    }

    #[test]
    fn a_refused_commit_puts_the_chat_in_the_queue_saying_why() {
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let hooks = Hooks::listening_on(
            plane,
            &at,
            Arc::new(move |moved| tx.lock().unwrap().send(moved).unwrap()),
            Arc::new(|_| panic!("a refused commit is not a harness started by hand")),
        )
        .expect("listening");
        hooks.board().opened(3, Some(Harness::ClaudeCode), None);
        let said = "commit refused in app: a.py:2  an email address  ad**";

        purlis_core::hookwire::deliver_refused(
            hooks.socket().expect("a socket"),
            Some(&hooks.token_for(3)),
            &purlis_core::hookwire::CommitRefused {
                chat: 3,
                commit_refused: said.to_owned(),
            },
        )
        .expect("told");

        let moved = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the window is told");
        assert_eq!(moved.session, 3);
        assert!(moved.needs_you);
        assert_eq!(moved.queue, vec![3]);
        assert_eq!(moved.refusals, vec![said.to_owned()]);
    }

    #[test]
    fn a_word_that_is_no_harness_charter_starts_puts_nothing_on_the_tab() {
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");

        let told = by_hand(
            &plane,
            StartedByHand {
                chat: 3,
                started_by_hand: "vim".to_owned(),
                cwd: None,
            },
        );

        assert_eq!(told, None);
    }

    use purlis_core::harness::Harness;
    use purlis_core::hookwire::Conversation;

    /// A board holding chat 7 running `harness`, under `conversation` where charter chose one.
    fn running(harness: Harness, conversation: Option<&str>) -> Hooks {
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks = Hooks::deaf(plane);
        hooks
            .board()
            .opened(7, Some(harness), conversation.map(str::to_owned));
        hooks
    }

    /// The conversation chat 7 moved onto when `conversation` was reported from `pid`, if any.
    fn followed_after(
        hooks: &Hooks,
        conversation: Conversation,
        pid: Option<u32>,
    ) -> Option<String> {
        let report = Report {
            agent: None,
            chat: 7,
            event: purlis_core::state::Event::UserPromptSubmit,
            conversation,
            pid,
            detail: purlis_core::state::Detail::default(),
        };
        apply(
            &hooks.board,
            &hooks.plane,
            &report,
            purlis_core::state::Waits::default(),
        )
        .followed
    }

    fn named(id: &str) -> Conversation {
        Conversation::Named(id.to_owned())
    }

    #[test]
    fn a_codex_chat_follows_the_first_conversation_its_harness_names_and_no_other() {
        let hooks = running(Harness::Codex, None);

        assert_eq!(
            followed_after(&hooks, named("aaa"), None).as_deref(),
            Some("aaa")
        );
        assert_eq!(
            followed_after(&hooks, named("aaa"), None),
            None,
            "no rewrite"
        );
        assert_eq!(
            followed_after(&hooks, named("bbb"), None),
            None,
            "a later id from a pid-less harness is a nested run, never followed"
        );
    }

    #[test]
    fn an_opencode_chat_follows_the_first_conversation_its_plugin_names() {
        let hooks = running(Harness::Opencode, None);

        assert_eq!(
            followed_after(&hooks, named("ses_abc"), None).as_deref(),
            Some("ses_abc")
        );
    }

    #[test]
    fn a_report_that_is_not_the_chat_s_own_harness_moves_no_conversation() {
        // ADR 0024 C5: a harness nested in the chat's shell must not be able to rewrite what
        // the chat resumes at the next launch.
        let codex = running(Harness::Codex, None);
        assert_eq!(
            followed_after(&codex, Conversation::Contradicted, None),
            None
        );
        assert_eq!(
            followed_after(&codex, named("ccc"), Some(99)),
            None,
            "a claude inside a codex chat"
        );
        assert_eq!(
            followed_after(&codex, named("aaa"), None).as_deref(),
            Some("aaa")
        );
        assert_eq!(followed_after(&codex, Conversation::Foreign, None), None);

        let claude = running(Harness::ClaudeCode, Some("chosen"));
        assert_eq!(followed_after(&claude, named("chosen"), Some(10)), None);
        assert_eq!(
            followed_after(&claude, named("nested"), Some(11)),
            None,
            "a claude started inside a claude chat"
        );
        assert_eq!(
            followed_after(&claude, Conversation::Contradicted, Some(10)),
            None
        );
    }

    #[test]
    fn a_claude_chat_follows_its_own_process_onto_the_conversation_a_clear_starts() {
        let hooks = running(Harness::ClaudeCode, Some("chosen"));
        assert_eq!(
            followed_after(&hooks, named("chosen"), Some(10)),
            None,
            "the id charter chose is already recorded; adopting it rewrites nothing"
        );

        assert_eq!(
            followed_after(&hooks, named("cleared"), Some(10)).as_deref(),
            Some("cleared")
        );
    }

    #[test]
    fn a_line_spooled_while_no_host_listened_is_recorded_when_the_project_is_opened_again() {
        use purlis_core::eventlog::{self, ArgsKey, Log, Recorder};
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join(".charter/app/hooks.sock"),
        };
        let plane: PlaneId =
            serde_json::from_value(serde_json::json!(dir.path())).expect("a plane id");
        let call = purlis_core::hookwire::ToolCall {
            chat: 7,
            tool_hook: "posttooluse".to_owned(),
            tool: Some("Read".to_owned()),
            call: Some("toolu_1".to_owned()),
            args: None,
            decision: purlis_core::hookwire::Decision::None,
            rule: None,
            hook_ms: 1,
            agent: None,
            at_ms: 0,
        };
        // The app issued chat 7 its token and quit; the chat's hook spooled its call.
        {
            let gone = Hooks::listening_on(plane.clone(), &at, Arc::new(|_| {}), Arc::new(|_| {}))
                .expect("listening");
            let token = gone.token_for(7);
            gone.stop();
            let delivered = purlis_core::hookwire::deliver_tool(&at.socket, Some(&token), &call)
                .expect("spooled");
            assert_eq!(delivered, purlis_core::hookwire::Delivered::Spooled(1));
        }

        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let logs = dir.path().join("events");
        hooks.record_into(Arc::new(Mutex::new(Recorder::new(
            Log::open(&logs, "DEVICE").expect("a log"),
            ArgsKey::open(&logs).expect("a key"),
        ))));
        hooks.drain_spool();

        let kinds: Vec<String> = eventlog::read(&logs)
            .expect("the log reads")
            .into_iter()
            .map(|event| event.kind)
            .collect();
        assert_eq!(kinds, ["hook.posttooluse", "hook.spool.drained"]);
    }

    /// A chat's spool key lasts as long as the chat (V99i): what it spooled while the host was
    /// busy is recorded as it closes, and a line under its key after that is rejected.
    #[test]
    fn a_closed_chats_spool_is_drained_into_the_log_and_its_key_does_not_outlive_it() {
        use purlis_core::eventlog::{self, ArgsKey, Log, Recorder};
        use purlis_core::hookwire::spool;
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join(".charter/app/hooks.sock"),
        };
        let plane: PlaneId =
            serde_json::from_value(serde_json::json!(dir.path())).expect("a plane id");
        let call = |chat: u32| purlis_core::hookwire::ToolCall {
            chat,
            tool_hook: "posttooluse".to_owned(),
            tool: Some("Read".to_owned()),
            call: Some("toolu_1".to_owned()),
            args: None,
            decision: purlis_core::hookwire::Decision::None,
            rule: None,
            hook_ms: 1,
            agent: None,
            at_ms: 0,
        };
        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let logs = dir.path().join("events");
        hooks.record_into(Arc::new(Mutex::new(Recorder::new(
            Log::open(&logs, "DEVICE").expect("a log"),
            ArgsKey::open(&logs).expect("a key"),
        ))));
        let spooling = spool::dir_for(&at.socket);
        let keys = || std::fs::read_to_string(spooling.join(spool::KEYS)).expect("the keys");
        let (closing, staying) = (hooks.token_for(7), hooks.token_for(8));
        // Each chat's hook spooled a call the host did not take in time.
        spool::append(&spooling, 7, &closing, &call(7)).expect("spooled");
        spool::append(&spooling, 8, &staying, &call(8)).expect("spooled");
        let both = keys();

        hooks.chat_ended(7);

        let kinds = || -> Vec<String> {
            eventlog::read(&logs)
                .expect("the log reads")
                .into_iter()
                .map(|event| event.kind)
                .collect()
        };
        assert_eq!(kinds(), ["hook.posttooluse", "hook.spool.drained"]);
        let id = |token: &purlis_core::hookwire::ChatToken| spool::SpoolKey::of(token).id();
        assert!(both.contains(&id(&closing)) && both.contains(&id(&staying)));
        assert!(!keys().contains(&id(&closing)), "{}", keys());
        assert!(keys().contains(&id(&staying)), "{}", keys());

        // A hook of the closed chat that is still about spools once more. Then the project is
        // opened again, with no reopen record: chat 8 is not brought back, so it has ended too.
        spool::append(&spooling, 7, &closing, &call(7)).expect("spooled");
        hooks.drain_spool();

        assert_eq!(
            kinds()[2..],
            [
                "hook.spool.rejected",
                "hook.posttooluse",
                "hook.spool.drained"
            ]
        );
        assert!(!keys().contains(&id(&staying)), "{}", keys());
    }

    #[test]
    fn the_run_a_clear_begins_is_told_with_the_conversation_it_moved_to() {
        // ADR 0066's `clear`: the event log begins the run, and the record has to hold it as
        // the chat's current run, so it is told beside the conversation.
        use purlis_core::eventlog::{self, ArgsKey, Log, Recorder};
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let logs = dir.path().join("events");
        hooks.record_into(Arc::new(Mutex::new(Recorder::new(
            Log::open(&logs, "DEVICE").expect("a log"),
            ArgsKey::open(&logs).expect("a key"),
        ))));
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        hooks.when_it_follows(Arc::new(move |chat, id, run| {
            let _ = tx
                .lock()
                .unwrap()
                .send((chat, id.to_owned(), run.map(str::to_owned)));
        }));
        hooks
            .board()
            .opened(7, Some(Harness::ClaudeCode), Some("chosen".to_owned()));
        let socket = hooks.socket().expect("a socket");
        let token = hooks.token_for(7);
        let say = |conversation: &str| {
            purlis_core::hookwire::send(
                socket,
                Some(&token),
                &Report {
                    chat: 7,
                    event: purlis_core::state::Event::UserPromptSubmit,
                    conversation: named(conversation),
                    pid: Some(10),
                    agent: None,
                    detail: Default::default(),
                },
            )
            .expect("sent");
        };

        say("chosen");
        say("cleared");

        let (chat, id, run) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the move is told");
        assert_eq!((chat, id.as_str()), (7, "cleared"));
        let cleared: Vec<_> = eventlog::read(&logs)
            .expect("the log reads")
            .into_iter()
            .filter(|event| event.kind == "run.started" && event.body["cause"] == "clear")
            .map(|event| event.run)
            .collect();
        assert_eq!(cleared, vec![run], "the run the log began for the clear");
    }

    #[test]
    fn a_clear_begins_a_run_the_record_is_told_even_with_no_event_log() {
        // #856 review F5: the run is the host's, so `reopen.json` moves with it either way.
        let dir = tempfile::tempdir().expect("a directory");
        let at = Where {
            within: dir.path().to_path_buf(),
            socket: dir.path().join("app").join("hooks.sock"),
        };
        let plane: PlaneId = serde_json::from_str("\"/plane\"").expect("a plane id");
        let hooks =
            Hooks::listening_on(plane, &at, Arc::new(|_| {}), Arc::new(|_| {})).expect("listening");
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        hooks.when_it_follows(Arc::new(move |_, id, run| {
            let _ = tx
                .lock()
                .unwrap()
                .send((id.to_owned(), run.map(str::to_owned)));
        }));
        hooks
            .board()
            .opened(7, Some(Harness::ClaudeCode), Some("chosen".to_owned()));
        let token = hooks.token_for(7);
        for conversation in ["chosen", "cleared"] {
            purlis_core::hookwire::send(
                hooks.socket().expect("a socket"),
                Some(&token),
                &Report {
                    chat: 7,
                    event: purlis_core::state::Event::UserPromptSubmit,
                    conversation: named(conversation),
                    pid: Some(10),
                    agent: None,
                    detail: Default::default(),
                },
            )
            .expect("sent");
        }

        let (id, run) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the move is told");
        assert_eq!(id, "cleared");
        assert!(
            run.as_deref()
                .and_then(purlis_core::reopen::a_ulid)
                .is_some(),
            "{run:?}"
        );
    }
}
