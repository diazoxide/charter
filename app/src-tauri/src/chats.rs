//! The chats the app has open: the sessions, plus what each one was started as.
//!
//! `Sessions` knows how to run a program in a terminal and nothing about why. This knows
//! why: which harness a session is, what conversation it is under, and which one is in
//! front — everything a quit has to write down and a launch has to put back.
//!
//! It is a layer of its own so that the record is written from what the app itself did, and
//! not from anything a harness said. Nothing here reads a session's output.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use purlis_core::engine::Size;
use purlis_core::eventlog::{Began, RunOf};
use purlis_core::harness::{Harness, SessionId};
use purlis_core::reopen::{Chat, Focus, Record, Reopened, View};

use purlis_core::harness::StateHooks;

use crate::host::{Opening, SessionHost};
use crate::sessions::{Reporting, Sessions};

/// Every identity variable a vault of the plane at `cwd` declares — both halves of each `env`
/// binding — so a chat is started without any of them (#271 review, U6). A `cwd` outside a
/// plane, or a registry that cannot be read, yields none: the chat still loses every `OP_*` by
/// prefix. Read on the thread that starts the chat; it is a small JSON read, once per start.
///
/// **Skipped in a fenced build.** Resolving the plane walks up from `cwd` and, in a fenced test
/// build, that walk aborts the moment it names a plane outside the fixture fence
/// (`purlis_core::fence`, charter-app#129) — which a unit test's `cwd` routinely does. A test
/// build therefore strips only by the `OP_` prefix; the declared-name strip is exercised at the
/// session builder ([`crate::sessions`] tests pass `env_strip` directly) and in the core.
fn declared_identity_vars(cwd: Option<&std::path::Path>) -> Vec<String> {
    if purlis_core::fence::FENCED {
        return Vec::new();
    }
    let Some(root) = cwd.and_then(|c| purlis_core::plane::find_root(c).ok()) else {
        return Vec::new();
    };
    let ctx = purlis_core::secrets::Ctx::new(&root, purlis_core::secrets::Env::from_process());
    let Ok(doc) = purlis_core::secrets::registry::load_registry(&ctx) else {
        return Vec::new();
    };
    let mut names: Vec<String> = purlis_core::secrets::registry::identity_vars(&doc)
        .into_iter()
        .flat_map(|(_, vars)| vars)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// What more of this machine's environment the operator lets a chat started in `cwd` have:
/// the `[chat_env] pass` of that plane's `charter.local.toml`. A chat outside a plane has none.
fn operator_env_pass(cwd: Option<&std::path::Path>) -> Vec<String> {
    cwd.and_then(|c| purlis_core::plane::find_root(c).ok())
        .map(|root| purlis_core::chatenv::read(&root))
        .unwrap_or_default()
}

/// One chat the app has open, as the UI and the quit warning see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    pub session: u32,
    pub name: String,
    pub cwd: Option<PathBuf>,
    pub harness: Option<Harness>,
    /// The harness profile it started on, and the persona it adopted — what the sidebar
    /// names a chat by, beside its harness.
    pub profile: Option<String>,
    pub persona: Option<String>,
    /// Whether it is the chat in front. At a launch this is the one that was in front when
    /// the app was quit, so the window comes back looking as it was left.
    pub in_front: bool,
    /// How it came to be open. A chat the operator just started is `Fresh`, the same as one
    /// that could not be resumed — the difference is only interesting at a relaunch, which
    /// is where the UI says it.
    pub how: Reopened,
    /// Whether the operator pinned it (ADR 0039). It rides the record, so a pinned
    /// chat comes back pinned; see [`purlis_core::reopen::Chat::pinned`].
    pub pinned: bool,
    /// The name the operator gave it, where they gave one (charter-app#254). It rides the
    /// record too; see [`purlis_core::reopen::Chat::label`].
    pub label: Option<String>,
    /// The chat a handoff opened it from, where one did (charter-app#258). It rides the
    /// record; see [`purlis_core::reopen::Chat::from`].
    pub from: Option<purlis_core::reopen::HandedFrom>,
}

/// Who an open chat is beyond this launch, and the directory it works in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAt {
    /// Its ULID (ADR 0066), `None` only for a chat that has not been given one.
    pub id: Option<String>,
    pub cwd: Option<PathBuf>,
}

/// The most chats one record may start at a launch. The product's scale is fifty (the
/// spec's limits table); this is only a backstop against a record nobody meant.
const MOST_AT_ONCE: usize = 200;

/// Where the record goes whenever what is open changes.
///
/// It is a callback rather than a file so that this module keeps knowing nothing about the
/// plane — and so a test can see exactly when a write would happen.
pub type Recorder = Box<dyn Fn(&Record) + Send + Sync>;

/// Told as each chat starts, BEFORE its program does: its number, the harness it runs and the
/// conversation charter chose for it. Everything the board needs to judge a report about it.
pub type Starting = Box<dyn Fn(u32, Option<Harness>, Option<String>) + Send + Sync>;

/// Told as each chat starts, BEFORE its program does, of the run the start begins: the chat's
/// number, its id, the run's id and why it began (ADR 0066). What the event log is told.
pub type Beginning = Box<dyn Fn(u32, RunOf<'_>, Began) + Send + Sync>;

/// What a start decided about a chat's sandbox (ADR 0067 §7): the trust event to write, under
/// the chat and the run the start is about to begin, BEFORE its program runs; or, once a new
/// chat has started, how it counts towards this machine's opt-out rate (ruling V78 d). Each
/// call carries one of the two.
pub struct Sandboxing<'a> {
    pub change: Option<&'a purlis_core::sandbox::Change>,
    /// The chat's id and the run it begins, for `change`.
    pub run: Option<RunOf<'a>>,
    pub counted: Option<purlis_core::sandbox::local::Started>,
    pub harness: Option<Harness>,
    pub persona: Option<&'a str>,
}

/// Told of [`Sandboxing`]. An `Err` is that the trust event was not written: a person's opt-out
/// is then not started, so no opt-out ever runs unaudited (ADR 0067 §7).
pub type Trusting = Box<dyn Fn(Sandboxing<'_>) -> Result<(), String> + Send + Sync>;

/// Why a chat is being started, which is what decides why its run begins (ADR 0066).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// A chat that was not open before: its first run, `start`.
    New,
    /// A chat a relaunch put back from the record: `reopen` or `fresh`, by how it came back
    /// ([`Began::at_relaunch`]).
    Relaunch,
    /// A chat a relaunch put back that its record held no id for, so one was minted at this
    /// launch: `reopen`, whatever it came back as. ADR 0066's migration: "Its first run after
    /// the upgrade has `cause: reopen`."
    Upgrade,
    /// A chat started again in the place of one whose harness could not bring its
    /// conversation back (`lostOnResume`): the same chat, `fresh`.
    Again,
}

/// The most chats that were closed this launch whose ids are remembered, for
/// [`Chats::start_ready_instead_of`]. That asks about a chat the window closed a moment ago.
const LET_GO_HELD: usize = 256;

use purlis_core::reopen::mint as minted;

/// One chat the app has open: what it was started as, how it came back, and the harness it
/// actually runs.
///
/// The harness is KEPT rather than asked of the chat again, because asking means asking its
/// program's NAME, and a profile's command is commonly a wrapper. The board is told this
/// same value at the start, so the sidebar and the board cannot disagree about what a chat
/// is running — which they did: the board learned the profile's declared kind while the
/// sidebar read `claude-stand-in` and said "no harness".
#[derive(Debug)]
struct Running {
    chat: Chat,
    how: Reopened,
    harness: Option<Harness>,
    /// What runs beside a chat charter wraps — its egress proxy and its own temp directory —
    /// for as long as the chat is open (ADR 0067 §2). Dropped with it.
    #[allow(dead_code)]
    confinement: Option<purlis_core::sandbox::Confinement>,
}

/// A chat a launch could not start, as the window lists it (NO-3): by its id, which Retry now
/// and Forget name it by, with its name and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct NotStarted {
    /// The chat's id (ADR 0066), stable across launches and never shared.
    pub id: String,
    /// What its tab was called. Two waiting chats can share one.
    pub name: String,
    /// Why it did not start.
    pub why: String,
    /// The approval its profile needs before it can start, where it needs one (#1246,
    /// D-1246-5): read when its start was last refused, so the window offers **Review and
    /// approve…** from this and never from the words of `why`. Null for a chat on no profile,
    /// a profile not declared, or one with nothing to approve.
    pub approval: Option<NeedsApproval>,
}

/// **A profile's command waiting on the operator's approval** (ADR 0022), as a waiting chat's
/// Notice offers it (#1246): which profile, and the exact line the approval is for.
///
/// What the picker's row says of the same profile (`ProfileRow`), in its words, so the window
/// draws the picker's own sentence and mark (`ProfileApproval.tsx`, ruling V69). `shown` is the
/// line `approve_profile` checks the click against: a file changed since this was read is
/// refused there, and nothing is recorded.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct NeedsApproval {
    /// The profile's name, as `approve_profile` takes it.
    pub profile: String,
    /// Its `kind`.
    pub kind: String,
    /// Where it was declared: `built-in`, `charter.local.toml` or the project's harnesses.
    pub source: String,
    /// `new` or `changed`, as the picker's row says it.
    pub approval: String,
    /// What it would run, as the picker shows it (`profiletrust::shown`), already contained.
    pub shown: String,
}

impl NeedsApproval {
    /// What `chat`'s profile needs approved at `root` before it can start, read through the
    /// launch's own read, as `approve_profile` reads it: a profile in a file the launch
    /// refuses has nothing a yes could buy.
    fn of(chat: &Chat, root: &std::path::Path) -> Option<Self> {
        let name = chat.profile.as_deref()?;
        let (set, _check) = purlis_core::profiles::for_launch(root);
        let profile = set.get(name)?;
        let needs = purlis_core::profiletrust::approval_needed(root, profile)?;
        Some(Self {
            profile: profile.name.clone(),
            kind: profile.kind.clone(),
            source: profile.source.as_str().to_owned(),
            approval: needs.as_str().to_owned(),
            shown: purlis_core::profiletrust::shown(root, profile),
        })
    }
}

/// A chat a launch could not start, as it is held: the chat, why, and the approval its profile
/// needs ([`NeedsApproval`]).
#[derive(Debug)]
struct Waiting {
    chat: Chat,
    why: String,
    approval: Option<NeedsApproval>,
}

/// Every chat the app has open, and which of them is in front.
pub struct Chats {
    /// Whatever runs the sessions (FD-3). A trait object, so nothing here can reach past
    /// [`SessionHost`] to a pty: a chat layer that did would not compile against another host.
    sessions: Box<dyn SessionHost>,
    /// Told as each chat starts, before its program does.
    starting: Mutex<Option<Starting>>,
    /// Told of the run each start begins, before its program runs (ADR 0066).
    beginning: Mutex<Option<Beginning>>,
    /// Told what each start decided about the chat's sandbox (ADR 0067 §7).
    trusting: Mutex<Option<Trusting>>,
    /// What a start found to say on a chat's tab after the core's start had spoken: that its
    /// trust event could not be written. Taken by the window's start ([`Self::start_notes`]).
    late_notes: Mutex<HashMap<u32, Vec<String>>>,
    /// This device's id, the origin device of every chat minted here; `None` where the machine
    /// store has none to give (ADR 0031), which a chat records as `unknown`.
    device: Option<String>,
    /// The ids of the chats closed this launch, by number, for a start in one's place.
    let_go: Mutex<HashMap<u32, purlis_core::reopen::Identity>>,
    /// Told when a chat that was announced turned out not to start.
    #[allow(clippy::type_complexity)]
    never_started: Mutex<Option<Box<dyn Fn(u32) + Send + Sync>>>,
    /// What the app ships that a chat is armed with: its own `charter` and its own plugin.
    shipped: crate::Shipped,
    open: Mutex<HashMap<u32, Running>>,
    front: Mutex<Option<u32>>,
    /// Chats a launch could not start, and why. They are kept because the record has to
    /// keep them: a workspace directory that has moved, or a harness mid-reinstall, must
    /// not silently delete the chat on the next write.
    would_not_start: Mutex<Vec<Waiting>>,
    /// The tabs the window has open that hold a view rather than a chat, as it last said.
    ///
    /// **The window's to say and this layer's to write down**, and nothing else: a view has no
    /// session, so there is nothing here that could know one opened. They are held beside the
    /// chats only because the record is one file and is written whole, in one place, under
    /// [`Self::writing`] — a second writer of `reopen.json` would be two answers racing to disk.
    views: Mutex<Vec<View>>,
    /// The branch the window's sidebar is focused on — its cockpit (FM-5) — as it last said,
    /// or at a launch as the record had it. Held here for [`Self::views`]' reason.
    focus: Mutex<Option<Focus>>,
    /// The order the window's strip draws the chats in, by session, as it last said — or, at
    /// a launch, the order the record listed them in (ADR 0039, as amended by SI-6).
    ///
    /// **The window's to say, for the reason [`Self::views`] is**: the operator drags a tab
    /// there, and nothing here could know it moved. It is held here and not in the window
    /// because the record is written from here, and because a reloaded window asks this layer
    /// what is open ([`Self::open_now`]) and has to get the strip back in the order it left it.
    /// A chat it has not placed yet — one that has just started — comes after the ones it has
    /// ([`Self::in_order`]).
    order: Mutex<Vec<u32>>,
    record_it: Recorder,
    /// Held across building a record and handing it over, so two changes at once cannot
    /// write themselves out of order and leave the older one on disk.
    writing: Mutex<()>,
    /// Set while a record is being put back, so reading one does not write it again once
    /// for every chat in it — fifty chats would be fifty writes of the same file, at the
    /// one moment the app is being measured for cold start.
    putting_back: AtomicBool,
    /// Set when every chat is being ended, at quit or when the project is closed: from then on
    /// nothing writes the record ([`Self::write_it_down`]).
    ending: AtomicBool,
    /// The most chats one record may start: [`MOST_AT_ONCE`], but for a test of the bound,
    /// which would otherwise open that many terminals (#1139).
    most_at_once: usize,
    /// What a person let each chat do past its project's sandbox, from a block's Notice
    /// (#1342), by the chat's id: handed to that chat's starts alone, so a restart on its
    /// conversation keeps it and no other chat ever gets it. **In memory only**: a grant for
    /// one chat ends with the app.
    grants: Mutex<HashMap<String, Vec<ChatGrant>>>,
    /// What each chat is owed once its turn ends (#1342): a restart on its conversation, with
    /// each sentence it is to be told, in order, as its first message. Queued, so a second
    /// grant before the restart adds to the first rather than replacing it.
    owed: Mutex<HashMap<u32, Vec<String>>>,
    /// The chats being started again in their place right now (#1342): one restart at a time
    /// per chat, whichever asked for it.
    restarting: Mutex<std::collections::HashSet<u32>>,
}

/// One grant a person made for one chat (#1342), as Settings' Granted list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatGrant {
    pub what: purlis_core::sandbox::grant::What,
    /// When, in seconds since 1970.
    pub at: u64,
    /// The chat's name as its tab showed it then.
    pub chat: String,
}

/// The arguments and the environment that arm a chat's harness for that chat alone.
type Armed = (Vec<String>, Vec<(String, String)>);

/// A chat's sandbox, as a start decided it: compiled for its harness with the real program it
/// then runs (ruling V87g), or lifted and why — never both, and neither where its project has
/// not turned the sandbox on.
type Decided = (
    Option<(purlis_core::sandbox::Applied, String)>,
    Option<purlis_core::sandbox::Lifted>,
);

impl Chats {
    /// Chats whose record is written by `record_it` every time what is open changes.
    ///
    /// Quitting writes it too, but only a graceful quit reaches that: an app that is killed,
    /// or crashes, runs no exit handler. Writing as it goes means such an app comes back on
    /// the chats it had rather than on none.
    pub fn recorded_by(record_it: Recorder) -> Self {
        Self::recorded_by_reporting_to(record_it, None)
    }

    /// The same, with sessions that report what their harness does to `reporting`'s socket.
    pub fn recorded_by_reporting_to(record_it: Recorder, reporting: Option<Reporting>) -> Self {
        Self::on_host(record_it, Box::new(Sessions::reporting_to(reporting)))
    }

    /// The same, on `host` — whatever runs the sessions, which is [`Sessions`] in the app.
    pub fn on_host(record_it: Recorder, host: Box<dyn SessionHost>) -> Self {
        Self {
            sessions: host,
            starting: Mutex::new(None),
            beginning: Mutex::new(None),
            trusting: Mutex::new(None),
            late_notes: Mutex::new(HashMap::new()),
            device: None,
            let_go: Mutex::new(HashMap::new()),
            never_started: Mutex::new(None),
            shipped: crate::Shipped::default(),
            open: Mutex::new(HashMap::new()),
            front: Mutex::new(None),
            would_not_start: Mutex::new(Vec::new()),
            views: Mutex::new(Vec::new()),
            focus: Mutex::new(None),
            order: Mutex::new(Vec::new()),
            record_it,
            writing: Mutex::new(()),
            putting_back: AtomicBool::new(false),
            ending: AtomicBool::new(false),
            most_at_once: MOST_AT_ONCE,
            grants: Mutex::new(HashMap::new()),
            owed: Mutex::new(HashMap::new()),
            restarting: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// These chats, starting at most `most` from a record rather than [`MOST_AT_ONCE`].
    #[cfg(test)]
    fn starting_at_most(self, most: usize) -> Self {
        Self {
            most_at_once: most,
            ..self
        }
    }

    /// Chats nothing records — what the tests use when the record is not what they are about.
    pub fn new() -> Self {
        Self::recorded_by(Box::new(|_| {}))
    }

    /// Calls `tell` as each chat starts, BEFORE its program does, with everything the board
    /// needs in order to judge a report about it.
    pub fn when_one_starts(&self, tell: Starting) {
        *lock(&self.starting) = Some(tell);
    }

    /// Calls `tell` as each chat starts, BEFORE its program does, with the run that start
    /// begins: the chat's id, which a relaunch keeps, a new run's id, and why (ADR 0066).
    pub fn when_a_run_begins(&self, tell: Beginning) {
        *lock(&self.beginning) = Some(tell);
    }

    /// Calls `tell` with what each start decided about the chat's sandbox: its trust event as
    /// its run begins, before its program runs, and how a new chat counts once it has started
    /// (ADR 0067 §7, ruling V78).
    pub fn when_the_sandbox_is_decided(&self, tell: Trusting) {
        *lock(&self.trusting) = Some(tell);
    }

    /// Whether `session` is a shell tab at `root`: a chat on no profile running no harness,
    /// in that directory. Where SD-30's install command may be typed (ruling V78 c), never an
    /// agent's pane.
    pub fn is_shell_at(&self, session: u32, root: &std::path::Path) -> bool {
        let same = |a: &std::path::Path, b: &std::path::Path| {
            a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
        };
        lock(&self.open).get(&session).is_some_and(|one| {
            one.chat.profile.is_none()
                && one.harness.is_none()
                && one.chat.cwd.as_deref().is_some_and(|cwd| same(cwd, root))
        })
    }

    /// What chat `session`'s start found to say on its tab beyond the core's notices, once.
    pub fn start_notes(&self, session: u32) -> Vec<String> {
        lock(&self.late_notes).remove(&session).unwrap_or_default()
    }

    /// Says which device this is: the origin device of every chat minted from now on.
    pub fn on_device(&mut self, device: Option<String>) {
        self.device = device;
    }

    /// This device's id, as [`Self::on_device`] gave it, or `None` where the machine store has
    /// none. It names the work link log a chat's link goes in (ADR 0088 §3).
    pub fn device(&self) -> Option<&str> {
        self.device.as_deref()
    }

    /// Who the chat in `session` is and where it works, or `None` for a session charter does
    /// not have open.
    pub fn chat_at(&self, session: u32) -> Option<ChatAt> {
        let open = lock(&self.open);
        let one = open.get(&session)?;
        Some(ChatAt {
            id: one.chat.identity.id.clone(),
            cwd: one.chat.cwd.clone(),
        })
    }

    /// Calls `tell` when a chat that was announced never started after all.
    ///
    /// The announcement has to come before the program, so a program that then fails to start
    /// leaves the board holding a chat that does not exist. Nothing is misattributed — ids are
    /// never reused — but it is one entry per failed start for the life of the app.
    pub fn when_one_does_not_start(&self, tell: Box<dyn Fn(u32) + Send + Sync>) {
        *lock(&self.never_started) = Some(tell);
    }

    /// Refuses every chat start while `switch` is thrown (OV-1), at the sessions every start
    /// reaches.
    pub fn stopped_by(&mut self, switch: std::sync::Arc<crate::killswitch::KillSwitch>) {
        self.sessions.stopped_by(switch);
    }

    /// The sessions underneath, for everything that is about a terminal and not about a chat.
    pub fn sessions(&self) -> &dyn SessionHost {
        self.sessions.as_ref()
    }

    /// What the app ships that a chat is armed with: the `charter` binary a hook runs and the
    /// plugin a Claude Code chat loads, when the app knows where each is.
    ///
    /// Its own, because the app and both ship together: the `charter` on `PATH` may be an
    /// older install, or the Python charter, and a hook pointed at either would be answering a
    /// different program's idea of these events.
    pub fn arming_with(&mut self, shipped: crate::Shipped) {
        self.shipped = shipped;
    }

    /// The arguments and the environment that arm this harness on this session alone, if any.
    ///
    /// `cwd` is the chat's own directory, which decides whether charter may also fill Claude
    /// Code's status line for it (`purlis_core::footerclaim`): project settings are read from
    /// the session's own directory, so that is the directory the question is asked about.
    ///
    /// `plugins` is the harness's own plugins the project chose for this chat
    /// (`purlis_core::start::Ready::plugins`, charter-app#274); empty for a chat on no profile.
    ///
    /// `sandbox` is the sandbox the core compiled for this chat (ADR 0067), or none.
    ///
    /// **Fail closed.** A sandbox compiled for another harness, or one the harness is not armed
    /// to carry here (the app shipped without its plugin or its binary), refuses the chat rather
    /// than starting it without the sandbox.
    fn state_hooks(
        &self,
        harness: Option<Harness>,
        cwd: Option<&std::path::Path>,
        plugins: &purlis_core::harness_plugin::Chosen,
        sandbox: Option<&purlis_core::sandbox::Applied>,
    ) -> Result<Armed, String> {
        let not_carried = "this plane runs every chat sandboxed, and this app cannot hand the \
                           sandbox to this chat's harness, so nothing was started";
        if let Some(applied) = sandbox
            && harness != Some(applied.harness())
        {
            return Err(format!("{not_carried}."));
        }
        let (Some(harness), Some(binary)) = (harness, self.shipped.binary.as_deref()) else {
            return match sandbox {
                Some(_) => Err(format!("{not_carried}: purlis's own binary was not found.")),
                None => Ok((Vec::new(), Vec::new())),
            };
        };
        let kit = purlis_core::harness::Kit {
            binary,
            plugin: self.shipped.plugin.as_deref(),
        };
        match harness.state_hooks(kit, cwd, plugins, sandbox) {
            StateHooks::ThisSessionOnly { args, env, .. } => Ok((args, env)),
            StateHooks::None if sandbox.is_some() => Err(format!(
                "{not_carried}: purlis's plugin, which carries it, was not found."
            )),
            // Nothing is added to the command line, and nothing of the operator's is written
            // behind their back. The chat shows `unknown`.
            StateHooks::None => Ok((Vec::new(), Vec::new())),
        }
    }

    /// The sandbox a chat that is not on a profile starts under: the same decision
    /// `purlis_core::start::ready` makes for one that is (ADR 0067). A chat whose program is a
    /// harness, in a plane that turned the sandbox on, is sandboxed or refused. A shell, or a
    /// chat outside any plane, is the operator's own and is left as it was.
    /// With the sandbox, the program the chat then runs: the real file the check asked about
    /// (ruling V87g), never the name it was recorded by.
    ///
    /// `opt_out` is a person's choice to run it without the sandbox for this run, made in the
    /// window on a sandbox block's Notice (#1342), and none for every other start. A system with
    /// no backend (Windows) starts it unsandboxed, and says why, as it does a chat on a profile.
    /// `grants` are what a person let this one chat do besides.
    fn sandbox_off_profile(
        chat: &Chat,
        grants: &purlis_core::sandbox::grant::Grants,
        opt_out: Option<&purlis_core::sandbox::OptOut>,
    ) -> Result<Decided, String> {
        let Some(harness) = chat.harness() else {
            return Ok((None, None));
        };
        // A `charter.toml` above the chat that is not a regular file is no plane to the walk
        // below, which would start the chat unsandboxed: it is refused instead.
        if chat
            .cwd
            .as_deref()
            .is_some_and(purlis_core::sandbox::marker_unreadable)
        {
            return Err(purlis_core::sandbox::NotStarted::PlaneUnreadable.to_string());
        }
        let Some(root) = chat
            .cwd
            .as_deref()
            .and_then(|cwd| purlis_core::plane::find_root(cwd).ok())
        else {
            return Ok((None, None));
        };
        let decided = purlis_core::sandbox::decide_granted(
            harness,
            &root,
            &purlis_core::sandbox::Machine::this(),
            &purlis_core::sandbox::backend::installed,
            opt_out,
            grants,
        )
        .map_err(|refused| refused.to_string())?;
        let (applied, lifted) = match decided {
            Some(purlis_core::sandbox::Decided::Sandboxed(applied)) => (Some(applied), None),
            Some(purlis_core::sandbox::Decided::Unsandboxed(lifted)) => (None, Some(lifted)),
            None => (None, None),
        };
        // Ruling V87g: the program is the harness the sandbox was compiled for, and not a file
        // a sandboxed chat could have changed. The real file it names is what the check asks;
        // a chat whose program then runs by another name would not be the one checked. A chat
        // that starts without the sandbox is bound to none.
        if let Some(applied) = &applied {
            let launch = chat.launch();
            let words = purlis_core::programs::resolve_argv(std::slice::from_ref(&launch.program))
                .map_err(|gone| format!("{} Nothing was started.", gone.said()))?;
            let cwd = chat.cwd.clone().unwrap_or_else(|| root.clone());
            let mut writable = applied.writable();
            writable.extend(purlis_core::sandbox::program::temp_roots(&[]));
            let checked = purlis_core::sandbox::program::checked(
                harness,
                &words,
                applied.root(),
                purlis_core::sandbox::program::Chat {
                    cwd: &cwd,
                    writable: &writable,
                    env: &[],
                },
                None,
            )
            .map_err(|refused| refused.to_string())?;
            return Ok((Some((applied.clone(), checked[0].clone())), None));
        }
        Ok((None, lifted))
    }

    /// Starts a chat the core has already worked out the launch for — a chat on a profile.
    ///
    /// The harness, the arguments and the environment all come from `ready`, which resolved
    /// them from the profile's DECLARED kind. Nothing here asks the program's name what it
    /// is: a profile's command is commonly a wrapper, and the answer would be `None`.
    pub fn start_ready(
        &self,
        chat: &Chat,
        ready: &purlis_core::start::Ready,
        size: Size,
    ) -> Result<u32, String> {
        self.start_ready_as(chat, ready, size, Why::New)
    }

    /// [`Self::start_ready`], in the place of chat `instead_of`, whose harness could not bring
    /// its conversation back (`lostOnResume`): **the same chat**, under its id, in a run that
    /// begins `fresh` (ADR 0066). `instead_of` may already be closed, as the window closes its
    /// tab first. A chat this launch never had is started as a new one.
    pub fn start_ready_instead_of(
        &self,
        instead_of: u32,
        chat: &Chat,
        ready: &purlis_core::start::Ready,
        size: Size,
    ) -> Result<u32, String> {
        // Taken out of what is recorded here, whether or not the window's close has arrived
        // yet: the two are not ordered, and a record holding both would name two chats by one
        // id (#856 review F3). Its program is the close's to end.
        let taken = lock(&self.open).remove(&instead_of);
        if let Some(taken) = &taken {
            lock(&self.let_go).insert(instead_of, taken.chat.identity.clone());
        }
        let was = taken
            .map(|one| one.chat.identity)
            .or_else(|| lock(&self.let_go).get(&instead_of).cloned());
        let Some(was) = was.filter(|was| was.id.is_some()) else {
            return self.start_ready(chat, ready, size);
        };
        let again = Chat {
            identity: purlis_core::reopen::Identity { run: None, ..was },
            ..chat.clone()
        };
        self.start_ready_as(&again, ready, size, Why::Again)
    }

    fn start_ready_as(
        &self,
        chat: &Chat,
        ready: &purlis_core::start::Ready,
        size: Size,
        why: Why,
    ) -> Result<u32, String> {
        self.open_it(
            chat,
            ready.program.clone(),
            ready.command.clone(),
            ready.args.clone(),
            ready.env.clone(),
            ready.harness,
            ready.session.as_ref().map(ToString::to_string),
            ready.how.clone(),
            &ready.plugins,
            ready.sandbox.as_ref(),
            ready.unsandboxed.as_ref(),
            size,
            // A chat on a profile is an agent, never the operator's shell.
            false,
            why,
        )
    }

    /// [`Self::put_back`] against a plane the test does not care about — every chat in
    /// these records is a shell, which is resolved from the record alone.
    #[cfg(test)]
    fn put_back_here(&self, record: &Record, size: Size) -> Vec<Open> {
        self.put_back(record, std::path::Path::new("/nonexistent-plane"), size)
    }

    /// Starts one chat out of the record, on its own profile where it had one.
    ///
    /// **The profile is looked up again**, never taken from the record: an edit to it takes
    /// effect at this launch rather than a stale copy running, and a profile that is gone
    /// means this chat is skipped BY NAME — another profile may be another account, where
    /// this chat's resume id does not exist and where its workspace's code was never meant
    /// to go. It stays in the record, so declaring the profile again brings it back.
    fn start_recorded(
        &self,
        chat: &Chat,
        root: &std::path::Path,
        size: Size,
        why: Why,
    ) -> Result<u32, String> {
        self.start_recorded_told(chat, root, size, why, None, None)
    }

    /// [`Self::start_recorded`], with `told` as the chat's first message where there is one: a
    /// sentence purlis tells it (#1342), last on its line, where its harness takes one
    /// (`handoff::first_message_argv`). A chat on no profile is started without it.
    fn start_recorded_told(
        &self,
        chat: &Chat,
        root: &std::path::Path,
        size: Size,
        why: Why,
        told: Option<&str>,
        opt_out: Option<purlis_core::sandbox::OptOut>,
    ) -> Result<u32, String> {
        let Some(profile) = chat.profile.clone() else {
            return self.start_as_opted(chat, size, false, why, opt_out.as_ref());
        };
        let ready = purlis_core::start::ready(
            &purlis_core::start::Start {
                profile: Some(profile),
                persona: chat.persona.clone(),
                name: chat.name.clone(),
                cwd: chat.cwd.clone(),
                resume: chat.resume.clone(),
                // The chat's own footer choice, brought back with it. It rides on the
                // environment, which is rebuilt at every start, so a relaunch that did not
                // carry it would silently blank a footer the operator had turned on.
                show_footer: chat.show_footer,
                resuming: None,
                without_sandbox: opt_out,
                grants: self.grants_of(chat),
            },
            root,
        )?;
        // The core's start knows no chat, so it says "nothing recorded"; this chat may know
        // better — a workspace rename left it without its conversation (charter#367).
        let mut ready = purlis_core::start::Ready {
            how: chat.told(ready.how.clone()),
            ..ready
        };
        // Last on the line, as a handoff's brief is: nothing may come after a positional prompt.
        if let Some(first) = told.and_then(|told| {
            ready
                .harness
                .and_then(|harness| purlis_core::handoff::first_message_argv(harness.name(), told))
        }) {
            ready.args.extend(first);
        }
        self.start_ready_as(chat, &ready, size, why)
    }

    /// What a person let `chat` do past its sandbox (#1342), by its id: none for a chat with no
    /// id, which no grant can have been made for.
    fn grants_of(&self, chat: &Chat) -> purlis_core::sandbox::grant::Grants {
        let mut grants = purlis_core::sandbox::grant::Grants::default();
        if let Some(id) = chat.identity.id.as_deref()
            && let Some(made) = lock(&self.grants).get(id)
        {
            for one in made {
                grants.add(&one.what);
            }
        }
        grants
    }

    /// **Lets chat `session` do `what`** (#1342), for as long as the app holds it, and owes it a
    /// restart on its conversation that tells it `told`. A chat that is not open, or one with no
    /// id, gets nothing, and is told why.
    pub fn grant(
        &self,
        session: u32,
        what: purlis_core::sandbox::grant::What,
        at: u64,
        told: String,
    ) -> Result<(), String> {
        let (id, name) = {
            let open = lock(&self.open);
            let running = open
                .get(&session)
                .ok_or_else(|| format!("chat {session} is not open, so nothing was allowed"))?;
            let id = running.chat.identity.id.clone().ok_or_else(|| {
                "that chat has no id yet, so nothing was allowed for it".to_owned()
            })?;
            (
                id,
                running
                    .chat
                    .label
                    .clone()
                    .unwrap_or_else(|| running.chat.name.clone()),
            )
        };
        let mut grants = lock(&self.grants);
        let made = grants.entry(id).or_default();
        if !made.iter().any(|one| one.what == what) {
            made.push(ChatGrant {
                what,
                at,
                chat: name,
            });
        }
        drop(grants);
        self.owe_restart(session, told);
        Ok(())
    }

    /// Owes chat `session` a restart on its conversation that tells it `told` (#1342): a grant
    /// of any level reaches a running chat that way.
    /// A chat that is not open is owed nothing: what was allowed reaches it at its next start.
    pub fn owe_restart(&self, session: u32, told: String) {
        if !lock(&self.open).contains_key(&session) {
            return;
        }
        lock(&self.owed).entry(session).or_default().push(told);
    }

    /// The folder chat `session` was started in: what a write grant for it is judged against.
    pub fn folder_of(&self, session: u32) -> Option<std::path::PathBuf> {
        lock(&self.open).get(&session)?.chat.cwd.clone()
    }

    /// Every grant made for an open chat, by chat id (#1348).
    pub fn chat_grants(&self) -> Vec<(String, ChatGrant)> {
        let open_ids: std::collections::HashSet<String> = lock(&self.open)
            .values()
            .filter_map(|running| running.chat.identity.id.clone())
            .collect();
        let mut out: Vec<(String, ChatGrant)> = lock(&self.grants)
            .iter()
            .filter(|(id, _)| open_ids.contains(*id))
            .flat_map(|(id, made)| made.iter().map(|one| (id.clone(), one.clone())))
            .collect();
        out.sort_by_key(|(_, one)| one.at);
        out
    }

    /// Whether chat `id` holds a grant of `what`.
    pub fn holds(&self, id: &str, what: &purlis_core::sandbox::grant::What) -> bool {
        lock(&self.grants)
            .get(id)
            .is_some_and(|made| made.iter().any(|one| one.what == *what))
    }

    /// Takes back chat `id`'s grant of `what` (#1348): its next start is compiled without it.
    /// Answers whether there was one.
    pub fn revoke(&self, id: &str, what: &purlis_core::sandbox::grant::What) -> bool {
        let mut grants = lock(&self.grants);
        let Some(made) = grants.get_mut(id) else {
            return false;
        };
        let before = made.len();
        made.retain(|one| one.what != *what);
        made.len() != before
    }

    /// **Restarts chat `session` on its conversation** (#1342, spike #1347): the same chat,
    /// under its id, its conversation resumed, so its new start compiles in what it was granted
    /// and its first message is what it was owed. Refused for a chat owed nothing. The old one
    /// stays open until the new one has started, as [`Self::start_fresh`]'s does; ending it is
    /// the caller's next step.
    pub fn restart_owed(
        &self,
        session: u32,
        root: &std::path::Path,
        size: Size,
    ) -> Result<u32, String> {
        self.claim(session)?;
        // Taken under the lock, so a grant made while this restart runs queues for the next one
        // and is never erased by this one.
        let Some(told) = lock(&self.owed).remove(&session) else {
            self.unclaim(session);
            return Err(format!(
                "purlis did not restart chat {session}: it is owed no restart."
            ));
        };
        let started = self.again_on_its_conversation(session).and_then(|again| {
            if again.resume.is_none() {
                return Err(format!(
                    "purlis did not restart chat {session}: it has no conversation to resume \
                     yet. What was allowed reaches it when it next starts."
                ));
            }
            self.start_recorded_told(
                &again,
                root,
                size,
                Why::Relaunch,
                Some(&told.join("\n\n")),
                None,
            )
        });
        match started {
            Ok(started) => self.restarted(session, started, true),
            Err(why) => {
                // Owed still: put back ahead of anything queued since.
                let mut owed = lock(&self.owed);
                let mut back = told;
                back.extend(owed.remove(&session).unwrap_or_default());
                owed.insert(session, back);
                drop(owed);
                self.unclaim(session);
                Err(why)
            }
        }
    }

    /// **Starts chat `session` again without the sandbox** (#1342): the person's own choice, from
    /// a block's Notice that purlis grants nothing for. It is the picker's opt-out (ADR 0067 §7),
    /// for this one chat and this one run: audited as `trust.sandbox.off` by the start, never
    /// recorded, so a later start of the chat is sandboxed again. Its conversation is resumed
    /// where it has one. It restarts now, mid-turn or not: the person pressed for it.
    pub fn restart_without_sandbox(
        &self,
        session: u32,
        root: &std::path::Path,
        size: Size,
    ) -> Result<u32, String> {
        self.claim(session)?;
        let opt_out = purlis_core::sandbox::OptOut {
            reason: Some(
                "started again without the sandbox from a sandbox block's Notice".to_owned(),
            ),
        };
        let started = self.again_on_its_conversation(session).and_then(|again| {
            self.start_recorded_told(&again, root, size, Why::Relaunch, None, Some(opt_out))
        });
        match started {
            // What was allowed and not yet taken is dropped: it means nothing to a chat run
            // without the sandbox, and would restart it sandboxed once its turn ended.
            Ok(started) => self.restarted(session, started, false),
            Err(why) => {
                self.unclaim(session);
                Err(why)
            }
        }
    }

    /// Marks chat `session` as restarting, or refuses: one restart at a time, whichever asked.
    fn claim(&self, session: u32) -> Result<(), String> {
        if lock(&self.restarting).insert(session) {
            Ok(())
        } else {
            Err(format!(
                "purlis is already starting chat {session} again, so it was not started twice."
            ))
        }
    }

    fn unclaim(&self, session: u32) {
        lock(&self.restarting).remove(&session);
    }

    /// What follows a restart of chat `session` as `started`: if the chat was closed while it
    /// restarted, the new run is ended (nobody asked for it any more, and it would keep the
    /// chat's grants alive unseen); otherwise the new run takes its place, and what was queued
    /// for the old one meanwhile is owed to the new one where `keep_owed`, else dropped.
    fn restarted(&self, session: u32, started: u32, keep_owed: bool) -> Result<u32, String> {
        if !lock(&self.open).contains_key(&session) {
            lock(&self.owed).remove(&session);
            self.unclaim(session);
            if let Err(why) = self.close(started) {
                tracing::warn!("purlis: a restart of a closed chat did not end ({why})");
            }
            return Err(format!(
                "purlis ended the new run of chat {session}: the chat was closed while it \
                 started again."
            ));
        }
        if let Some(queued) = lock(&self.owed).remove(&session)
            && keep_owed
        {
            lock(&self.owed).insert(started, queued);
        }
        self.took_the_place_of(session, started);
        self.unclaim(session);
        Ok(started)
    }

    /// Chat `session` as it would start again on its conversation: the same chat under its id,
    /// in a new run.
    fn again_on_its_conversation(&self, session: u32) -> Result<Chat, String> {
        let was = lock(&self.open)
            .get(&session)
            .map(|one| one.chat.clone())
            .ok_or_else(|| format!("purlis did not restart chat {session}: it is not open."))?;
        Ok(Chat {
            identity: purlis_core::reopen::Identity {
                run: None,
                ..was.identity.clone()
            },
            pid: None,
            number: None,
            ..was
        })
    }

    /// `started` takes the place of `session` in the strip's order, and the record says so.
    fn took_the_place_of(&self, session: u32, started: u32) {
        for placed in lock(&self.order).iter_mut() {
            if *placed == session {
                *placed = started;
            }
        }
        self.write_it_down();
    }

    /// The chats owed a restart to take a grant (#1342): what the window drives once each one's
    /// turn has ended, read again whenever the window is drawn anew.
    pub fn owed_restarts(&self) -> Vec<u32> {
        let mut owed: Vec<u32> = lock(&self.owed).keys().copied().collect();
        owed.sort_unstable();
        owed
    }

    /// Starts a chat, and remembers what it was started as.
    ///
    /// For a chat that is NOT on a profile — the operator's shell — where what runs is
    /// decided from the record alone.
    pub fn start(&self, chat: &Chat, size: Size) -> Result<u32, String> {
        self.start_as(chat, size, false, Why::New)
    }

    /// [`Self::start`], for the shell the operator opens from the window: the one start the
    /// kill switch lets through while agents are stopped (OV-1, ADR 0071).
    pub fn start_operator_shell(&self, chat: &Chat, size: Size) -> Result<u32, String> {
        self.start_as(chat, size, true, Why::New)
    }

    fn start_as(
        &self,
        chat: &Chat,
        size: Size,
        operator_shell: bool,
        why: Why,
    ) -> Result<u32, String> {
        self.start_as_opted(chat, size, operator_shell, why, None)
    }

    /// [`Self::start_as`], with a person's opt-out of the sandbox for this run where there is
    /// one (#1342's "Start without the sandbox" on a block's Notice).
    fn start_as_opted(
        &self,
        chat: &Chat,
        size: Size,
        operator_shell: bool,
        why: Why,
        opt_out: Option<&purlis_core::sandbox::OptOut>,
    ) -> Result<u32, String> {
        // Before anything is resolved or run, as for a chat on a profile.
        let (sandboxed, unsandboxed) =
            Self::sandbox_off_profile(chat, &self.grants_of(chat), opt_out)?;
        let mut launch = chat.launch();
        let sandbox = sandboxed.map(|(applied, program)| {
            launch.program = program;
            applied
        });
        // A shell tab's shims, and the start files that keep them first. Never recorded: they
        // are this build's, and worked out again at every start.
        let (args, env) = self.shell_start(chat, &launch.program, launch.args);
        self.open_it(
            chat,
            launch.program,
            // A chat on no profile runs its program by name, so there is no wrapper's
            // command to keep in front: its recorded words follow charter's, as they always
            // have, because they may end in a positional prompt.
            Vec::new(),
            args,
            env,
            chat.harness(),
            launch.session.as_ref().map(ToString::to_string),
            launch.how,
            // A chat on no profile has no project choice to carry: it runs as it always did,
            // with the pins alone.
            &std::collections::BTreeMap::new(),
            sandbox.as_ref(),
            unsandboxed.as_ref(),
            size,
            operator_shell,
            why,
        )
    }

    /// What a shell tab's shell is started with beyond `args`, the chat's own words: charter's
    /// shims first on its `PATH`, and the start files that keep them first (ADR 0062). A chat
    /// that is not a shell tab — one on a profile, or one running a harness — gets nothing,
    /// and nor does any chat in an app that has no shims.
    fn shell_start(
        &self,
        chat: &Chat,
        program: &str,
        args: Vec<String>,
    ) -> (Vec<String>, Vec<(String, String)>) {
        let Some(shims) = self.shipped.shims.as_ref() else {
            return (args, Vec::new());
        };
        // A shell tab is a chat on no profile running no harness — `open_session` with no
        // program, and the same chat put back from the record.
        if chat.profile.is_some() || chat.harness().is_some() {
            return (args, Vec::new());
        }
        // The `PATH` every chat gets, worked out by the one function that works it out, with
        // the shims put in front of it.
        let chat_env =
            purlis_core::start::with_chat_path(Vec::new(), self.shipped.binary.as_deref());
        let start = shims.shell_start(
            program,
            chat_env,
            std::env::var_os("PATH").as_deref(),
            std::env::var_os("ZDOTDIR").as_deref(),
        );
        let mut all = start.args;
        all.extend(args);
        (all, start.env)
    }

    /// What arms a chat's git with charter's hooks (SQ-16): `core.hooksPath` pointed at the
    /// app's hooks directory, for a chat running a harness, so every commit its agent makes —
    /// in a workspace repo, a piece, or a repository outside any plane — is scanned before it
    /// is made. Nothing for a shell tab, which is the operator's own, and nothing when the app
    /// could not write the hooks.
    pub(crate) fn git_hooks_for(
        &self,
        harness: Option<Harness>,
    ) -> Option<purlis_core::githooks::GitHooks> {
        harness.and(self.shipped.git_hooks.clone())
    }

    /// The one place a session is opened and a chat is remembered.
    ///
    /// Everything that differs between a profile chat and a shell chat is decided by the
    /// caller and arrives here as arguments — above all the HARNESS, which for a profile
    /// comes from its declared kind and must not be asked of the program's name.
    #[allow(clippy::too_many_arguments)]
    fn open_it(
        &self,
        chat: &Chat,
        program: String,
        command: Vec<String>,
        args: Vec<String>,
        env: Vec<(String, String)>,
        harness: Option<Harness>,
        conversation: Option<String>,
        how: purlis_core::reopen::Reopened,
        plugins: &purlis_core::harness_plugin::Chosen,
        sandbox: Option<&purlis_core::sandbox::Applied>,
        unsandboxed: Option<&purlis_core::sandbox::Lifted>,
        size: Size,
        operator_shell: bool,
        why: Why,
    ) -> Result<u32, String> {
        // Who the chat is, and the run this start begins (ADR 0066). **The id is minted once**
        // per clone and device (V43: a copy's was minted again before `put_back` got it),
        // when no record holds one, on this device, and a chat put back or started again keeps
        // the one it had, with its origin device. Every start begins a run of its own.
        let identity = match &chat.identity.id {
            Some(_) => purlis_core::reopen::Identity {
                run: Some(minted()),
                ..chat.identity.clone()
            },
            None => purlis_core::reopen::Identity {
                id: Some(minted()),
                device: self.device.clone(),
                run: Some(minted()),
                resumed_from: chat.identity.resumed_from.clone(),
            },
        };
        let cause = match why {
            Why::New => Began::Start,
            Why::Relaunch => Began::at_relaunch(&how, harness.is_some()),
            Why::Upgrade => Began::Reopen,
            Why::Again => Began::Fresh,
        };
        // What this start means for the sandbox's audit and its count (ADR 0067 §7): off where
        // it starts without the sandbox, back on where its last run did and this one does not,
        // and one more new chat towards the opt-out rate.
        let (trust, counted) = purlis_core::sandbox::at_start(
            unsandboxed,
            sandbox.is_some(),
            chat.unsandboxed,
            why == Why::New,
        );
        // The trust event, written before anything runs. **An opt-out is never unaudited**
        // (ADR 0067 §7): a person's is not started when it cannot be written. A system with no
        // backend still starts — every chat there would otherwise be refused — and its tab
        // says the record is missing.
        let mut late = Vec::new();
        if let Some(change) = &trust {
            let written = match (
                lock(&self.trusting).as_ref(),
                identity.id.as_deref(),
                identity.run.as_deref(),
            ) {
                (Some(trusting), Some(id), Some(run)) => trusting(Sandboxing {
                    change: Some(change),
                    run: Some(RunOf { chat: id, run }),
                    counted: None,
                    harness,
                    persona: chat.persona.as_deref(),
                }),
                _ => Err("purlis's event log is not open".to_owned()),
            };
            if let Err(why) = written {
                use purlis_core::sandbox::{By, Change, Lifted};
                match change {
                    Change::Off(Lifted { by: By::Person, .. }) => {
                        return Err(format!(
                            "purlis could not record that this chat would run without the \
                             sandbox ({why}), so it was not started. An opt-out is always \
                             recorded; start it sandboxed, or try again once the event log is \
                             back."
                        ));
                    }
                    Change::Off(_) => late.push(format!(
                        "purlis could not record that this chat runs without the sandbox \
                         ({why}), so the event log has no record of it."
                    )),
                    // Back on: the chat is sandboxed, and the record that it is can be missing.
                    Change::On => tracing::warn!(
                        "purlis: a chat's sandbox came back on and was not recorded ({why})"
                    ),
                }
            }
        }
        // The profile's own command first — a wrapper reads its own words before it hands the
        // rest on (M8.3) — then the state hooks, then charter's own words: a chat's recorded
        // arguments may end in a positional prompt that nothing may come after.
        // `purlis_core::start::Ready::command_line` is the one place that order is decided.
        let (hooks, armed) = self.state_hooks(harness, chat.cwd.as_deref(), plugins, sandbox)?;
        // What a wrapped chat needs running beside it, started before it and kept for as long
        // as it is open: charter's egress proxy and its own temp directory (ADR 0067 §2).
        let confinement = match sandbox {
            Some(applied) => applied.confine().map_err(|err| {
                format!(
                    "this plane runs every chat sandboxed, and purlis could not start what the \
                     sandbox needs beside this chat ({err}), so nothing was started."
                )
            })?,
            None => None,
        };
        // Under a sandbox, the sandbox decides the line: a flag of the harness's own in the
        // chat's own words can outrank it, so such a chat is refused here, where every chat
        // opens, and flags it rides on go last among the flags. A harness charter wraps runs as
        // the wrap's program, with its whole line after it (ADR 0067).
        let socket = self.sessions.reports_to();
        let (program, all, wrapped) = match sandbox {
            Some(applied) => {
                let line = applied.line(
                    purlis_core::sandbox::Words {
                        program,
                        command,
                        armed: hooks,
                        charters: args,
                    },
                    &purlis_core::sandbox::At {
                        cwd: chat.cwd.as_deref(),
                        hook_socket: socket.as_deref(),
                        confinement: confinement.as_ref(),
                    },
                )?;
                (line.program, line.args, line.env)
            }
            None => (
                program,
                purlis_core::start::Ready::line(command, hooks, args),
                Vec::new(),
            ),
        };
        let mut env = env;
        env.extend(armed);
        // What the wrap sets wins over a profile's or the arming's value of the same name: its
        // proxy, its temp directory. Sorting cannot decide it, since the session applies the
        // pairs in order and two of one name would leave the later one standing.
        env.retain(|(key, _)| !wrapped.iter().any(|(set, _)| set == key));
        env.extend(wrapped);
        what_its_hooks_read(&mut env, chat.cwd.as_deref(), sandbox.is_some());
        env.sort();
        // The app's own `charter` first, then the directories charter searched for the
        // harness — so a hook the plane spells as the bare word `charter`, or a skill's
        // command, finds the one this app shipped, from a Finder launch too (charter-app#136).
        let env = purlis_core::start::with_chat_path(env, self.shipped.binary.as_deref());
        // What the announcement below said, so a start that fails can take it back.
        let announced = std::sync::atomic::AtomicU32::new(0);
        let session = self
            .sessions
            .open(
                // The number this chat already answers to, where it has one. A chat put
                // back keeps the key its workspace pointer and session lock are under; a
                // chat the operator just started has none yet (charter-app#90).
                chat.number,
                &Opening {
                    program: Some(program),
                    args: all,
                    cwd: chat.cwd.as_ref().map(|cwd| cwd.display().to_string()),
                    size,
                    env,
                    // Every identity variable a vault of this chat's plane declares, so none
                    // reaches the chat even when it is not `OP_`-prefixed (#271 review, U6). Read
                    // from the plane the chat starts in; a chat outside a plane declares none.
                    env_strip: declared_identity_vars(chat.cwd.as_deref()),
                    harness,
                    env_pass: operator_env_pass(chat.cwd.as_deref()),
                    operator_shell,
                    git_hooks: self.git_hooks_for(harness),
                },
                &|session| {
                    announced.store(session, std::sync::atomic::Ordering::SeqCst);
                    if let Some(starting) = lock(&self.starting).as_ref() {
                        starting(session, harness, conversation.clone());
                    }
                    if let Some(beginning) = lock(&self.beginning).as_ref()
                        && let (Some(chat), Some(run)) = (&identity.id, &identity.run)
                    {
                        beginning(session, RunOf { chat, run }, cause);
                    }
                },
            )
            .inspect_err(|_| {
                // The chat was announced and then did not start. Take it back, or the board
                // holds one entry per failed start for the life of the app.
                let announced = announced.load(std::sync::atomic::Ordering::SeqCst);
                if announced > 0
                    && let Some(gone) = lock(&self.never_started).as_ref()
                {
                    gone(announced);
                }
            })?;
        // Under the id it was actually given, not the one it was recorded with: a chat
        // started fresh is under an id the app just chose, and that is what has to be
        // written down for the next launch to resume it.
        // And no longer as a chat a workspace rename left without its conversation: it has
        // said so once, at this start, and is under a conversation of its own now.
        let under = Chat {
            resume: conversation
                .as_deref()
                .and_then(|id| purlis_core::harness::SessionId::new(id).ok()),
            renamed_from: None,
            // What this run started under, for the next run's audit and the session record —
            // never read as an opt-out by any start.
            unsandboxed: unsandboxed.is_some(),
            identity,
            ..chat.clone()
        };
        // Counted once it has started, so a start that failed is not a chat. A count that
        // could not be kept costs the rate one chat, never the chat.
        if let (Some(counted), Some(trusting)) = (counted, lock(&self.trusting).as_ref())
            && let Err(why) = trusting(Sandboxing {
                change: None,
                run: None,
                counted: Some(counted),
                harness,
                persona: chat.persona.as_deref(),
            })
        {
            tracing::warn!("purlis: a chat was not counted for the sandbox ({why})");
        }
        if !late.is_empty() {
            lock(&self.late_notes).insert(session, late);
        }
        lock(&self.open).insert(
            session,
            Running {
                chat: under,
                how,
                harness,
                confinement,
            },
        );
        self.write_it_down();
        Ok(session)
    }

    /// Ends a chat. It is no longer one a quit would record.
    pub fn close(&self, session: u32) -> Result<(), String> {
        let gone = lock(&self.open).remove(&session);
        lock(&self.owed).remove(&session);
        if let Some(gone) = gone {
            // A chat's grants end with it (D-1348-1): kept only while a session of that chat is
            // open, which a restart for a grant is, since it starts before the old one ends.
            if let Some(id) = gone.chat.identity.id.as_deref() {
                let still = lock(&self.open)
                    .values()
                    .any(|one| one.chat.identity.id.as_deref() == Some(id));
                if !still {
                    lock(&self.grants).remove(id);
                }
            }
            let mut let_go = lock(&self.let_go);
            if let_go.len() >= LET_GO_HELD {
                let_go.clear();
            }
            let_go.insert(session, gone.chat.identity);
        }
        let mut front = lock(&self.front);
        if *front == Some(session) {
            *front = None;
        }
        drop(front);
        let closed = self.sessions.close(session);
        self.write_it_down();
        closed
    }

    /// Which chat is in front, or none.
    pub fn front(&self) -> Option<u32> {
        *lock(&self.front)
    }

    /// Pins or unpins one chat, and writes the record so the pin outlives the app.
    ///
    /// **A chat charter does not have open cannot be pinned**, and the answer says so rather
    /// than inventing an entry: a pin is an arrangement of what is there, and the record is
    /// the only thing that says a chat exists at all (ADR 0040). It follows that a
    /// pinned chat that does not come back at a launch takes its pin with it, which is the
    /// dangling-pin question answered by there being nowhere for one to dangle.
    ///
    /// Nothing is written when nothing changed, for the reason `bring_to_front` gives: the
    /// record is rewritten on every write, and a write is a fingerprint the machine store
    /// then has to vouch for.
    pub fn pin(&self, session: u32, pinned: bool) -> Result<(), String> {
        let mut open = lock(&self.open);
        let Some(one) = open.get_mut(&session) else {
            return Err(format!("purlis has no chat {session} open to pin."));
        };
        if one.chat.pinned == pinned {
            return Ok(());
        }
        one.chat.pinned = pinned;
        drop(open);
        self.write_it_down();
        Ok(())
    }

    /// Gives a chat the name `raw`, or takes the one it was given off when `raw` is blank —
    /// and answers the name it now has (charter-app#254).
    ///
    /// **Charter's label and nothing else**: the harness keeps the name it was started with
    /// (`Chat::name`), so a rename never reaches a program that is running. The name is held to
    /// [`purlis_core::reopen::label`], and a refusal changes nothing and says why.
    ///
    /// Nothing is written when nothing changed, for [`Self::pin`]'s reason.
    pub fn rename(&self, session: u32, raw: &str) -> Result<Option<String>, String> {
        let label = purlis_core::reopen::label(raw)?;
        let mut open = lock(&self.open);
        let Some(one) = open.get_mut(&session) else {
            return Err(format!("purlis has no chat {session} open to rename."));
        };
        if one.chat.label == label {
            return Ok(label);
        }
        one.chat.label.clone_from(&label);
        drop(open);
        self.write_it_down();
        Ok(label)
    }

    /// The name `session` is shown under — the one it was given, or its default — or `None`
    /// for a chat charter does not have open (charter-app#258).
    pub fn shown_name(&self, session: u32) -> Option<String> {
        let open = lock(&self.open);
        let one = open.get(&session)?;
        Some(purlis_core::reopen::shown_name(
            &one.chat,
            one.harness.map(Harness::name),
        ))
    }

    /// The harness `session` was started as, or none for a shell or a chat charter does not
    /// have open — the same one [`Self::open_now`] answers, never one inferred from the
    /// program's name.
    pub fn harness(&self, session: u32) -> Option<Harness> {
        lock(&self.open).get(&session)?.harness
    }

    /// The handoff `session` was opened by, where one opened it.
    pub fn handed_from(&self, session: u32) -> Option<purlis_core::reopen::HandedFrom> {
        lock(&self.open).get(&session)?.chat.from.clone()
    }

    /// Records what `session` owes the chat that handed it off, and writes the record so it
    /// holds across a relaunch (charter-app#259). Nothing for a chat no handoff opened.
    pub fn owes(&self, session: u32, owed: purlis_core::reopen::Owed) {
        let mut open = lock(&self.open);
        let Some(from) = open
            .get_mut(&session)
            .and_then(|one| one.chat.from.as_mut())
        else {
            return;
        };
        if from.report == owed {
            return;
        }
        from.report = owed;
        drop(open);
        self.write_it_down();
    }

    /// Chat `session`'s own harness has put it in conversation `id` — the first one a Codex
    /// or opencode chat names, or the one a Claude Code chat moved to on `/clear` (C6) — so
    /// that is the conversation the record resumes it by from now on (Q10).
    ///
    /// **The board has already judged it.** This is told only what `Board::reported` adopted
    /// or followed (`Hooks::when_it_follows`), so a nested harness's report never gets here.
    ///
    /// Written down only when the record would change, for [`Self::pin`]'s reason. Nothing for
    /// a chat that runs no harness: a shell tab is resumed by nothing, whatever ran in it.
    ///
    /// **A report that arrives before the chat is remembered here is not kept**, which no
    /// harness charter starts can produce: Claude Code's first report names the id charter
    /// chose, which moves nothing, and Codex and opencode name theirs inside the first turn,
    /// long after the start returned.
    ///
    /// `run` is the run the host began when the move was a `/clear` (ADR 0066's `clear`): the
    /// chat's current run from now on, and so the one the record names.
    pub fn follow_conversation(&self, session: u32, id: &str, run: Option<&str>) {
        let Ok(id) = SessionId::new(id) else { return };
        let mut open = lock(&self.open);
        let Some(one) = open.get_mut(&session) else {
            return;
        };
        if one.harness.is_none() {
            return;
        }
        let mut moved = false;
        if one.chat.resume.as_ref() != Some(&id) {
            one.chat.resume = Some(id);
            moved = true;
        }
        if let Some(run) = run
            && one.chat.identity.run.as_deref() != Some(run)
        {
            one.chat.identity.run = Some(run.to_owned());
            moved = true;
        }
        drop(open);
        if moved {
            self.write_it_down();
        }
    }

    /// What the window says its view tabs are now. Written down when it differs from what was
    /// held, and not otherwise — for [`Self::pin`]'s reason: every write is a fingerprint the
    /// machine store then has to vouch for.
    pub fn hold_views(&self, views: Vec<View>) {
        let changed = std::mem::replace(&mut *lock(&self.views), views.clone()) != views;
        if changed {
            self.write_it_down();
        }
    }

    /// The view tabs the window last said it had — at a launch, the ones the record put back.
    pub fn views(&self) -> Vec<View> {
        lock(&self.views).clone()
    }

    /// What branch the window's sidebar is focused on now (FM-5), or `None` for the whole
    /// workspace. Written down when it differs from what was held, for [`Self::hold_views`]'
    /// reason.
    pub fn hold_focus(&self, focus: Option<Focus>) {
        let changed = std::mem::replace(&mut *lock(&self.focus), focus.clone()) != focus;
        if changed {
            self.write_it_down();
        }
    }

    /// The branch the window last said its sidebar was focused on — at a launch, the record's.
    pub fn focus(&self) -> Option<Focus> {
        lock(&self.focus).clone()
    }

    /// Follows a workspace rename in everything held here that names it (charter#367): a
    /// chat's directory, the workspace a handed-off chat came from, and each view tab's strip —
    /// and writes the record once if any of it moved.
    ///
    /// The core has already rewritten the record on disk; without this the next write would put
    /// the old name back, because this is what the record is written from.
    pub fn follow(&self, moved: &purlis_core::wscmd::rename::Move) {
        let mut changed = false;
        for one in lock(&self.open).values_mut() {
            changed |= moved.chat(&mut one.chat);
        }
        for one in lock(&self.would_not_start).iter_mut() {
            changed |= moved.chat(&mut one.chat);
        }
        for view in lock(&self.views).iter_mut() {
            changed |= moved.view(view);
        }
        if let Some(focus) = lock(&self.focus).as_mut() {
            changed |= moved.focus(focus);
        }
        if changed {
            self.write_it_down();
        }
    }

    /// What order the window's strip now draws the chats in, by session (SI-6).
    ///
    /// Written down when the order the record would list them in moves, and not otherwise —
    /// for [`Self::pin`]'s reason. Not when the list said differs from the one held: the window
    /// says it after every change to its tabs, and a chat it has just opened is already last.
    pub fn hold_order(&self, sessions: Vec<u32>) {
        let was = self.in_order();
        *lock(&self.order) = sessions;
        if self.in_order() != was {
            self.write_it_down();
        }
    }

    /// The running sessions in the strip's order: the ones the window placed, where it placed
    /// them, then any it has not placed yet in the order they were opened.
    ///
    /// **The one answer to "in what order"**, for both the record and a reloaded window, so the
    /// two cannot disagree about which tab comes first.
    fn in_order(&self) -> Vec<u32> {
        let running = self.sessions.running();
        let placed = lock(&self.order).clone();
        let mut ordered: Vec<u32> = placed
            .iter()
            .copied()
            .filter(|session| running.contains(session))
            .collect();
        ordered.extend(
            running
                .into_iter()
                .filter(|session| !placed.contains(session)),
        );
        ordered
    }

    /// Says which chat is in front, so the record knows which one to bring back in front.
    pub fn bring_to_front(&self, session: Option<u32>) {
        let changed = std::mem::replace(&mut *lock(&self.front), session) != session;
        if changed {
            self.write_it_down();
        }
    }

    /// What is open, in the strip's order.
    pub fn open_now(&self) -> Vec<Open> {
        // In the strip's order (`in_order`), so the window comes back with its tabs the way
        // they were left. Asked before `open` is held: it takes `order` itself.
        let ordered = self.in_order();
        let open = lock(&self.open);
        let front = *lock(&self.front);
        ordered
            .into_iter()
            .filter_map(|session| {
                let running = open.get(&session)?;
                let chat = &running.chat;
                Some(Open {
                    session,
                    name: chat.name.clone(),
                    cwd: chat.cwd.clone(),
                    // The harness this chat was STARTED as, not one inferred from its
                    // program's name — a profile's command is commonly a wrapper, and the
                    // sidebar used to answer "no harness" for one while the board knew the
                    // kind. One idea of what is running, or the two drift.
                    harness: running.harness,
                    profile: chat.profile.clone(),
                    persona: chat.persona.clone(),
                    in_front: front == Some(session),
                    how: running.how.clone(),
                    pinned: chat.pinned,
                    label: chat.label.clone(),
                    from: chat.from.clone(),
                })
            })
            .collect()
    }

    /// What was open, to write down.
    pub fn record(&self) -> Record {
        let ordered = self.in_order();
        // Asked before `open` is taken: a program being ended answers only once it is gone,
        // and nothing else may wait on `open` that long (R2-2).
        let pids: HashMap<u32, Option<u32>> = ordered
            .iter()
            .map(|&session| (session, self.sessions.process_id(session)))
            .collect();
        let open = lock(&self.open);
        let front = *lock(&self.front);
        // **One chat, once** (NO-3): a chat being retried is waiting and open at once until its
        // start returns, and one being started fresh is open twice, under its old session and
        // its new. The record holds each id once — as the newest session open under it.
        let mut newest: HashMap<&str, u32> = HashMap::new();
        for (&session, running) in open.iter() {
            if let Some(id) = running.chat.identity.id.as_deref() {
                let at = newest.entry(id).or_insert(session);
                *at = (*at).max(session);
            }
        }
        let superseded = |chat: &Chat, session: Option<u32>| {
            chat.identity
                .id
                .as_deref()
                .and_then(|id| newest.get(id))
                .is_some_and(|&at| session != Some(at))
        };
        // The ones that could not be started come first, in the order they were recorded,
        // so they keep their place and are tried again at the next launch.
        let mut chats: Vec<Chat> = lock(&self.would_not_start)
            .iter()
            .map(|one| &one.chat)
            .filter(|chat| !superseded(chat, None))
            .map(|chat| Chat {
                pid: None,
                ..chat.clone()
            })
            .collect();
        // Then the running ones, in the strip's order — which is the order the next launch
        // puts them back in.
        chats.extend(ordered.into_iter().filter_map(|session| {
            let chat = &open.get(&session)?.chat;
            if superseded(chat, Some(session)) {
                return None;
            }
            Some(Chat {
                active: front == Some(session),
                // The number it is actually running under, which is the key its workspace
                // pointer and session lock are written at. Taken from the session and not
                // from the chat, so the two can never come to say different things.
                number: Some(session),
                // The process it runs as now, which is what a commit is checked against (V82,
                // #1018) — never one a record put back carried from an earlier launch.
                pid: pids.get(&session).copied().flatten(),
                ..chat.clone()
            })
        }));
        Record {
            chats,
            views: lock(&self.views).clone(),
            focus: lock(&self.focus).clone(),
            // What the next launch must not deal again — charter-app#90. It is the high
            // water mark and not the count of what is open, so the numbers of chats that
            // were closed are spent too, and no new chat lands on a pointer one of them
            // left behind.
            dealt: self.sessions.dealt(),
            // An ordinary write, which is what makes the flag last one launch: the quit that
            // restarts charter for an update is the only writer that says otherwise (#251).
            relaunch_after_update: false,
            // The writer stamps the clone and device it writes from (V43, `Records::write`).
            clone_seat: None,
        }
    }

    /// Puts a record back: one session per chat it holds, resumed where it can be.
    ///
    /// A chat whose program cannot be started is left out and the rest still open — a
    /// relaunch that failed whole because one harness had been uninstalled would be worse
    /// than one that came back short.
    pub fn put_back(&self, record: &Record, root: &std::path::Path, size: Size) -> Vec<Open> {
        self.putting_back.store(true, Ordering::SeqCst);
        // Before a single chat starts, so that a number the record spent on a chat it no
        // longer holds — one the operator closed before quitting — is not dealt again to a
        // chat that would then read its workspace pointer and take its lock
        // (charter-app#90). The chats below raise the counter past their own numbers as they
        // go; this is the part of it no chat in the record can say.
        self.sessions.already_dealt(record.dealt);
        // The view tabs start nothing, so they are simply held until the window asks for them
        // (`reopened_views`) — and written back out with everything else at the next change.
        *lock(&self.views) = record.views.clone();
        *lock(&self.focus) = record.focus.clone();
        // Every chat here starts a program, synchronously, before it has a pane. A
        // record with thousands in it — a runaway, or a file nobody meant — would give an
        // app that hangs on launch with no way to intervene. The cap is far above the
        // fifty the product is for, so it never meets an operator; it is only ever a
        // backstop. What it leaves out stays recorded, like anything else that did not
        // start.
        //
        // A chat the record holds no id for is given one here, before it is tried, so the
        // one that does not start is kept under it too and is not given another at every
        // launch it fails at (#856 review F2).
        //
        // **And so is a chat whose id another chat in the record already has** — a record
        // hand-edited, corrupt or left by an older bug. Two chats are two chats: the record writes
        // one per id, the newest open (`record`), which is right only for a retry or a fresh
        // start under way, and two running under one id would lose one at the next write.
        let mut seen = std::collections::HashSet::new();
        let chats: Vec<(Chat, Why)> = record
            .chats
            .iter()
            .map(|chat| match &chat.identity.id {
                Some(id) if seen.insert(id.clone()) => (chat.clone(), Why::Relaunch),
                _ => (
                    Chat {
                        identity: purlis_core::reopen::Identity {
                            id: Some(minted()),
                            device: self.device.clone(),
                            ..chat.identity.clone()
                        },
                        ..chat.clone()
                    },
                    Why::Upgrade,
                ),
            })
            .collect();
        let most = self.most_at_once;
        let (starting, too_many) = chats.split_at(chats.len().min(most));
        for (chat, _) in too_many {
            lock(&self.would_not_start).push(Waiting {
                chat: chat.clone(),
                why: format!("more than {most} chats were recorded"),
                // Never tried, so nothing was refused: Retry now reads it.
                approval: None,
            });
        }
        let mut front = None;
        let mut opened: Vec<u32> = Vec::new();
        for (chat, why) in starting {
            match self.start_recorded(chat, root, size, *why) {
                Ok(session) => {
                    if chat.active {
                        front = Some(session);
                    }
                    opened.push(session);
                }
                // Kept, not dropped: the next record has to hold it too, or a directory
                // that has moved deletes the chat for good.
                Err(why) => {
                    // Read before the lock is taken, as Retry does: it reads the profile's
                    // file and runs git, and the list must not wait on either.
                    let approval = NeedsApproval::of(chat, root);
                    lock(&self.would_not_start).push(Waiting {
                        approval,
                        chat: chat.clone(),
                        why,
                    });
                }
            }
        }
        self.bring_to_front(front);
        // The strip comes back in the record's order, which is the order it was drawn in when
        // it was written — not the order of the numbers the chats kept (charter-app#90).
        *lock(&self.order) = opened.clone();
        self.putting_back.store(false, Ordering::SeqCst);
        // Written ONCE, when every chat has been tried, and not once per chat (fifty writes of
        // one file at the moment cold start is measured). What came back holds what the file
        // did not: a run each, begun at this launch, and an id for a chat recorded before ids
        // (ADR 0066). An id only in memory is minted again after a crash (#856 review F1).
        // The chats that did not start are written too, first, as every write writes them.
        if !record.chats.is_empty() {
            self.write_it_down();
        }
        let open = self.open_now();
        open.into_iter()
            .filter(|one| opened.contains(&one.session))
            .collect()
    }

    /// Writes the record again because a chat's program ended on its own, so it no longer
    /// names that program's pid (V82, #1018). The chat stays, as its tab does.
    pub fn a_program_ended(&self) {
        self.write_it_down();
    }

    /// Hands `write` the last record, the one written at quit or when the project is closed:
    /// the chats are kept for the next launch, with no pid, because their programs are ended
    /// right after this (V82, #1018).
    ///
    /// **Nothing is written after it.** Writing stops BEFORE this record is built, and it is
    /// built and written under the lock every write takes, so a write already on its way (a
    /// program that ended on its own, an operator's click) lands before it or not at all, and
    /// never puts live pids back on disk after it (R2-1).
    pub fn write_last(&self, write: impl FnOnce(&Record)) {
        self.ending.store(true, Ordering::SeqCst);
        let _writing = lock(&self.writing);
        let mut record = self.record();
        for chat in &mut record.chats {
            chat.pid = None;
        }
        write(&record);
    }

    /// Hands the record as it now is to whoever writes it.
    fn write_it_down(&self) {
        // Nothing is written once the quit has begun: its own write is the last, and a
        // program's end heard after it would write a record with no chats in it.
        if self.putting_back.load(Ordering::SeqCst) || self.ending.load(Ordering::SeqCst) {
            return;
        }
        // The record is built and handed over under one lock, so that two changes landing
        // together cannot write themselves out of order and leave the older one on disk.
        let _writing = lock(&self.writing);
        // Asked again under the lock: the last record may have been written while this one
        // waited for it ([`Self::write_last`]).
        if self.ending.load(Ordering::SeqCst) {
            return;
        }
        (self.record_it)(&self.record());
    }

    /// The chats a launch could not start, by id, name and reason. The window says so.
    ///
    /// **By id**, because a name says less than it seems to: a split's chat takes its tab's
    /// name and tab numbers start again at every launch, so two waiting chats can share one,
    /// and a Forget meant for the second must never drop the first (NO-3 review). Every waiting
    /// chat has an id: [`Self::put_back`] mints one before it tries a chat that had none.
    pub fn would_not_start(&self) -> Vec<NotStarted> {
        lock(&self.would_not_start)
            .iter()
            .map(|one| NotStarted {
                id: one.chat.identity.id.clone().unwrap_or_default(),
                name: one.chat.name.clone(),
                why: one.why.clone(),
                approval: one.approval.clone(),
            })
            .collect()
    }

    /// **Retry now** on a chat a launch could not start (NO-3): starts it again the way the
    /// launch did ([`Self::start_recorded`], as a relaunch), and answers its session.
    ///
    /// **Never twice, never lost.** The chat stays in the waiting list while it starts, so a
    /// record written meanwhile still has it; [`Self::record`] leaves out a waiting chat whose id
    /// is open, so one written after the start has it once, as running. It leaves the list only
    /// once it has started, and if it fails again it stays with the new reason.
    pub fn retry(&self, id: &str, root: &std::path::Path, size: Size) -> Result<u32, String> {
        let chat = lock(&self.would_not_start)
            .iter()
            .find(|one| one.chat.identity.id.as_deref() == Some(id))
            .map(|one| one.chat.clone())
            .ok_or_else(|| format!("chat {id} is not waiting to start"))?;
        let started = self.start_recorded(&chat, root, size, Why::Relaunch);
        // Read again at every refusal, outside the lock: what the profile needs now, and not
        // what it needed at the launch (#1246).
        let approval = started
            .as_ref()
            .err()
            .and_then(|_| NeedsApproval::of(&chat, root));
        {
            let mut waiting = lock(&self.would_not_start);
            match &started {
                Ok(_) => waiting.retain(|one| one.chat.identity.id.as_deref() != Some(id)),
                Err(why) => {
                    for one in waiting.iter_mut() {
                        if one.chat.identity.id.as_deref() == Some(id) {
                            one.why = why.clone();
                            one.approval = approval.clone();
                        }
                    }
                }
            }
        }
        self.write_it_down();
        started
    }

    /// **Forget this chat** (NO-3): drops a chat a launch could not start from the record, by
    /// its id.
    ///
    /// The one way such a chat leaves it. It is kept on purpose otherwise, so that a directory
    /// that moved, or a harness mid-reinstall, does not delete it ([`Self::put_back`]).
    pub fn forget(&self, id: &str) -> Result<(), String> {
        {
            let mut waiting = lock(&self.would_not_start);
            let at = waiting
                .iter()
                .position(|one| one.chat.identity.id.as_deref() == Some(id))
                .ok_or_else(|| format!("chat {id} is not waiting to start"))?;
            waiting.remove(at);
        }
        self.write_it_down();
        Ok(())
    }

    /// **Start fresh** (NO-3, the plane-updated mark): chat `session` started again, on the
    /// plane as it is now, as **the same chat** under its id in a run that begins `fresh`, with
    /// no conversation resumed (ADR 0066).
    ///
    /// Its profile is looked up again, as at a relaunch ([`Self::start_recorded`]), so an edit
    /// to it is what the new run starts on. **The old one stays open until the new one has
    /// started**, so a refused start leaves it running and recorded as it was; while both are
    /// open, [`Self::record`] writes the newer. Ending the old one is the caller's next step
    /// (`Held::start_chat_fresh`), which takes it off the board too.
    pub fn start_fresh(
        &self,
        session: u32,
        root: &std::path::Path,
        size: Size,
    ) -> Result<u32, String> {
        let was = lock(&self.open)
            .get(&session)
            .map(|one| one.chat.clone())
            .ok_or_else(|| format!("chat {session} is not open"))?;
        let again = Chat {
            identity: purlis_core::reopen::Identity {
                run: None,
                ..was.identity.clone()
            },
            resume: None,
            pid: None,
            number: None,
            ..was
        };
        let started = self.start_recorded(&again, root, size, Why::Again)?;
        // **It keeps its place** (#1246): the window puts the new session in the old one's pane,
        // so the record puts it where the old one was in the strip's order. Unplaced, it would
        // go last, and the next launch would draw it at the end of the strip.
        for placed in lock(&self.order).iter_mut() {
            if *placed == session {
                *placed = started;
            }
        }
        self.write_it_down();
        Ok(started)
    }

    /// How many chats are remembered, which is not the same as how many are running: this
    /// is what a chat that closed has to stop costing. Only the tests ask.
    #[cfg(test)]
    pub fn remembered(&self) -> usize {
        lock(&self.open).len()
    }

    /// Ends every chat, and does not return until their programs are gone.
    pub fn end_all(&self) {
        self.ending.store(true, Ordering::SeqCst);
        self.sessions.end_all();
        lock(&self.open).clear();
        lock(&self.would_not_start).clear();
        lock(&self.views).clear();
        *lock(&self.focus) = None;
        *lock(&self.front) = None;
    }
}

impl Default for Chats {
    fn default() -> Self {
        Self::new()
    }
}

fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a chat's hooks read about it (#1338, #1345), put in its `env`: the folder it was started
/// in, and whether a sandbox was actually applied to it. Set where every chat opens, and only
/// there: whatever a profile or the arming said under either name is replaced.
fn what_its_hooks_read(env: &mut Vec<(String, String)>, cwd: Option<&Path>, sandboxed: bool) {
    let ours = [
        purlis_core::hookwire::SANDBOXED_ENV,
        purlis_core::sandboxblock::CHAT_DIR_ENV,
    ];
    env.retain(|(key, _)| !ours.contains(&key.as_str()));
    if let Some(cwd) = cwd {
        env.push((ours[1].to_owned(), cwd.display().to_string()));
    }
    if sandboxed {
        env.push((ours[0].to_owned(), "1".to_owned()));
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_chats_hooks_are_told_its_folder_and_only_a_sandboxed_chat_is_called_sandboxed() {
        let of = |env: &[(String, String)], name: &str| {
            env.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
        };
        let mut env = vec![
            ("PURLIS_SANDBOXED".to_owned(), "1".to_owned()),
            ("PATH".to_owned(), "/bin".to_owned()),
        ];
        what_its_hooks_read(&mut env, Some(Path::new("/plane/workspaces/a")), false);
        assert_eq!(
            of(&env, "PURLIS_SANDBOXED"),
            None,
            "a profile cannot claim it"
        );
        assert_eq!(
            of(&env, "PURLIS_CHAT_DIR").as_deref(),
            Some("/plane/workspaces/a")
        );
        what_its_hooks_read(&mut env, Some(Path::new("/plane/workspaces/a")), true);
        assert_eq!(of(&env, "PURLIS_SANDBOXED").as_deref(), Some("1"));
        assert_eq!(
            env.iter().filter(|(n, _)| n == "PURLIS_CHAT_DIR").count(),
            1
        );
        assert!(purlis_core::sandbox::chat_is_sandboxed_in(&|name| of(
            &env, name
        )));
    }

    #[test]
    fn a_chats_grants_are_its_own_queue_its_restart_and_end_when_it_closes() {
        // #1342 and #1348 (D-1348-1).
        use purlis_core::sandbox::grant::What;
        let chats = Chats::new();
        let size = Size {
            columns: 80,
            rows: 24,
        };
        let chat = Chat {
            program: "/bin/sleep".to_owned(),
            args: vec!["5".to_owned()],
            name: "granted".to_owned(),
            ..Default::default()
        };
        let session = chats.start(&chat, size).expect("started");
        let other = chats.start(&chat, size).expect("started");
        let host = What::Host(purlis_core::sandbox::hosts::Host::parse("a.example").unwrap());
        chats
            .grant(session, What::Write("/tmp/x".into()), 1, "first".to_owned())
            .expect("granted");
        chats
            .grant(session, host.clone(), 2, "second".to_owned())
            .expect("granted");

        assert_eq!(chats.chat_grants().len(), 2);
        assert_eq!(chats.owed_restarts(), [session]);
        // Queued, not replaced: the restart tells both.
        assert_eq!(lock(&chats.owed)[&session], ["first", "second"]);
        // Never another chat's.
        let id = |n: u32| {
            lock(&chats.open)[&n]
                .chat
                .identity
                .id
                .clone()
                .expect("an id")
        };
        assert!(chats.holds(&id(session), &host));
        assert!(!chats.holds(&id(other), &host));
        // A chat with no conversation yet is not restarted, and keeps what it is owed.
        let refused = chats
            .restart_owed(session, std::path::Path::new("/nonexistent"), size)
            .unwrap_err();
        assert!(refused.contains("no conversation to resume"), "{refused}");
        assert_eq!(chats.owed_restarts(), [session]);
        assert_eq!(
            lock(&chats.owed)[&session],
            ["first", "second"],
            "put back whole"
        );
        assert!(
            lock(&chats.restarting).is_empty(),
            "the claim is given back"
        );

        // One restart at a time: a second, from either way in, is refused while one runs, and
        // a grant made meanwhile queues rather than being lost.
        chats.claim(session).expect("claimed");
        let twice = chats
            .restart_owed(session, std::path::Path::new("/nonexistent"), size)
            .unwrap_err();
        assert!(twice.contains("already starting"), "{twice}");
        let twice = chats
            .restart_without_sandbox(session, std::path::Path::new("/nonexistent"), size)
            .unwrap_err();
        assert!(twice.contains("already starting"), "{twice}");
        chats.owe_restart(session, "third".to_owned());
        assert_eq!(lock(&chats.owed)[&session], ["first", "second", "third"]);
        chats.unclaim(session);

        let gone = id(session);
        chats.close(session).ok();
        assert!(chats.chat_grants().is_empty());
        assert!(
            !chats.holds(&gone, &host),
            "a closed chat's grants end with it"
        );
        assert!(chats.owed_restarts().is_empty());
        // A closed chat is owed nothing (a grant for you made from its Notice reaches its next
        // start instead).
        chats.owe_restart(session, "late".to_owned());
        assert!(chats.owed_restarts().is_empty());
        chats.close(other).ok();
    }

    #[test]
    fn a_chat_is_announced_before_its_program_starts() {
        // A harness fires `SessionStart` at its own exec, so anything that learned the chat's
        // number afterwards would miss it — and for a chat that is then idle, waiting for a
        // first prompt, no second event ever comes. That is every chat of a relaunch.
        //
        // The order is the whole point: the announcement must land before the program can
        // have run at all.
        use std::sync::mpsc;

        let chats = Chats::new();
        let (tx, rx) = mpsc::channel();
        chats.when_one_starts(Box::new(move |session, harness, conversation| {
            let _ = tx.send((session, harness, conversation));
        }));

        let session = chats
            .start(
                &Chat {
                    // A program that prints and stops at once: by the time `start` returns it
                    // may already be gone, so an announcement made afterwards could be too
                    // late even in this test.
                    program: "/bin/echo".to_owned(),
                    args: vec!["hello".to_owned()],
                    cwd: None,
                    name: "ide.7".to_owned(),
                    resume: None,
                    active: false,
                    profile: None,
                    persona: None,
                    show_footer: false,
                    pinned: false,
                    number: None,
                    label: None,
                    from: None,
                    renamed_from: None,
                    ..Default::default()
                },
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .expect("the chat starts");

        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(5)),
            Ok((session, None, None)),
            "the board was not told about the chat"
        );
    }

    #[test]
    fn a_chat_is_announced_before_its_program_could_have_run_a_single_byte() {
        // **The ORDER is the fix, and a surviving mutant proved the first test does not check
        // it**: moving the announcement after the spawn still delivers it, so the defect
        // could come back silently. Checking what the app had bookkept was no better — that
        // happens after the spawn either way.
        //
        // So the announcement WAITS, briefly, for something only a running program could
        // make. A program that has not been started cannot make it however long we wait; one
        // that has makes it in milliseconds. The wait is what turns an ordering into
        // something a test can see.
        use std::sync::{Arc, Mutex};

        let dir = tempfile::tempdir().expect("a directory");
        let mark = dir.path().join("the-program-ran");
        let seen = Arc::new(Mutex::new(None));

        let chats = Chats::new();
        chats.when_one_starts({
            let mark = mark.clone();
            let seen = Arc::clone(&seen);
            Box::new(move |_, _, _| {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(750);
                while std::time::Instant::now() < deadline && !mark.exists() {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                *seen.lock().expect("not poisoned") = Some(mark.exists());
            })
        });

        chats
            .start(
                &Chat {
                    program: "/bin/sh".to_owned(),
                    args: vec![
                        "-c".to_owned(),
                        format!("touch {}; sleep 30", mark.display()),
                    ],
                    cwd: None,
                    name: "ide.7".to_owned(),
                    resume: None,
                    active: false,
                    profile: None,
                    persona: None,
                    show_footer: false,
                    pinned: false,
                    number: None,
                    label: None,
                    from: None,
                    renamed_from: None,
                    ..Default::default()
                },
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .expect("the chat starts");

        assert_eq!(
            *seen.lock().expect("not poisoned"),
            Some(false),
            "the program had already run when the board was told about its chat"
        );
    }

    #[test]
    fn a_chat_that_was_announced_and_then_did_not_start_is_taken_back() {
        // The announcement must come before the program, so it can be about a chat that never
        // happens. Without taking it back, the board holds one entry per failed start for the
        // life of the app.
        use std::sync::{Arc, Mutex};

        let chats = Chats::new();
        let announced = Arc::new(Mutex::new(Vec::new()));
        let taken_back = Arc::new(Mutex::new(Vec::new()));
        chats.when_one_starts({
            let announced = Arc::clone(&announced);
            Box::new(move |session, _, _| announced.lock().expect("not poisoned").push(session))
        });
        chats.when_one_does_not_start({
            let taken_back = Arc::clone(&taken_back);
            Box::new(move |session| taken_back.lock().expect("not poisoned").push(session))
        });

        let refused = chats.start(
            &Chat {
                program: "/no/such/program/anywhere".to_owned(),
                args: Vec::new(),
                cwd: None,
                name: "ide.7".to_owned(),
                resume: None,
                active: false,
                profile: None,
                persona: None,
                show_footer: false,
                pinned: false,
                number: None,
                label: None,
                from: None,
                renamed_from: None,
                ..Default::default()
            },
            Size {
                columns: 80,
                rows: 24,
            },
        );

        assert!(refused.is_err(), "a program that is not there started");
        let announced = announced.lock().expect("not poisoned").clone();
        assert_eq!(announced.len(), 1, "it was never announced");
        assert_eq!(*taken_back.lock().expect("not poisoned"), announced);
    }

    #[test]
    fn a_chat_is_announced_with_the_harness_and_conversation_it_was_started_under() {
        // What the board needs in order to judge a report: which rulebook, and which
        // conversation charter chose. A `claude` nested in the chat's shell reports a
        // different one, and that is the whole of what keeps it out (ADR 0024, C5).
        use std::sync::mpsc;

        let chats = Chats::new();
        let (tx, rx) = mpsc::channel();
        chats.when_one_starts(Box::new(move |session, harness, conversation| {
            let _ = tx.send((session, harness, conversation));
        }));

        // `/bin/echo` named `claude` is what `Harness::of_command` reads, and it is the file
        // name that decides — so this is a Claude Code chat as far as the app is concerned.
        // Copied through `stand_in::copy_of`, not `fs::copy`: this chat runs it the moment it
        // is written, and a program this process copied through its own descriptor can lose
        // to `ETXTBSY` (charter-app#81).
        let dir = tempfile::tempdir().expect("a directory");
        let claude = stand_in::copy_of(std::path::Path::new("/bin/echo"), dir.path(), "claude");

        chats
            .start(
                &Chat {
                    program: claude.display().to_string(),
                    args: Vec::new(),
                    cwd: None,
                    name: "ide.7".to_owned(),
                    resume: None,
                    active: false,
                    profile: None,
                    persona: None,
                    show_footer: false,
                    pinned: false,
                    number: None,
                    label: None,
                    from: None,
                    renamed_from: None,
                    ..Default::default()
                },
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .expect("the chat starts");

        let (_, harness, conversation) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the board was told");
        assert_eq!(harness, Some(Harness::ClaudeCode));
        // Charter chooses Claude Code's id at the start, so the board has it before the
        // harness has said anything.
        assert!(
            conversation.is_some(),
            "the chosen conversation was not passed on"
        );
    }
    use purlis_core::harness::SessionId;
    use purlis_core::reopen::Fresh;

    use super::*;
    use crate::host::pretend::Pretend;

    const SIZE: Size = Size {
        columns: 80,
        rows: 24,
    };
    const ID: &str = "11111111-2222-4333-8444-555555555555";

    /// A directory holding a program called `claude` that prints the arguments it was given
    /// and then waits, so a test can see what the app actually put on its command line.
    fn a_claude(dir: &std::path::Path) -> String {
        // Through `stand_in::program`, which holds both halves of this. The rename is what
        // charter-app#39 needed: the stand-in ends in `sleep 600`, so an earlier chat still
        // has it open for execution, and writing a running program is ETXTBSY — measured at
        // 2 failures in 5 runs, and because this binary runs first, `cargo test` stopped and
        // every later test binary was SKIPPED. The write from a child is what charter-app#81
        // needed, in the other direction: a chat runs this the moment it is written.
        stand_in::program(
            dir,
            "claude",
            "#!/bin/sh\nprintf 'argv:'\nfor word in \"$@\"; do printf ' %s' \"$word\"; done\nprintf '\\n'\nsleep 600\n",
        )
        .display()
        .to_string()
    }

    fn chat(program: &str, name: &str, resume: Option<&str>) -> Chat {
        Chat {
            program: program.to_owned(),
            args: vec![],
            cwd: None,
            name: name.to_owned(),
            resume: resume.map(|id| SessionId::new(id).expect("a valid id in a test")),
            active: false,
            profile: None,
            persona: None,
            show_footer: false,
            pinned: false,
            number: None,
            label: None,
            from: None,
            renamed_from: None,
            ..Default::default()
        }
    }

    /// How long a chat's program may take to run its first line, which only a broken test
    /// waits out. A freshly written stand-in is slow to start on a loaded machine: measured at
    /// 1.3–6.5 s from the spawn at load 50–70, and past ten seconds at load 100 (#1138). The
    /// spawn itself returned in under 0.2 s, and the script needed only milliseconds once it ran.
    const TO_START: std::time::Duration = std::time::Duration::from_secs(60);

    /// How long a stand-in that is running may take to say what a test waits for. It counts
    /// from the stand-in's own first sign of life, never from the spawn, so how slowly the
    /// machine starts a program does not count against it.
    const ONCE_RUNNING: std::time::Duration = std::time::Duration::from_secs(10);

    /// Everything a session has printed, once `text` is among it — as a READER would see
    /// it, not as the terminal encoded it.
    ///
    /// Two things had to be got right here, and the second cost a CI round. **Wait for what
    /// you are about to assert**: the stand-in `claude` prints `argv:` as a write of its own
    /// and its arguments as later ones, so waiting for `argv:` returned before a single
    /// argument had arrived. And **match against the text, not the encoding**: what a view
    /// emits is a terminal's output — erase-to-end-of-line, carriage returns, a line wrapped
    /// at the pane's width — so a long argv is `--resume` then an escape then the rest, and
    /// no substring of the command line is present as contiguous bytes. Both spellings can
    /// report a failure that has not happened and miss one that has.
    fn until_printed(chats: &Chats, session: u32, text: &str) -> String {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        let seen = Arc::new(Mutex::new(String::new()));
        let collect = {
            let seen = Arc::clone(&seen);
            move |more: String| lock(&seen).push_str(&more)
        };
        chats
            .sessions()
            .watch(session, Box::new(collect))
            .expect("the view opens");
        // The terminal's own drawing arrives before the program has run at all, so the
        // program's first word on the screen is the sign it is running.
        let watched = Instant::now();
        let mut running_since = None;
        loop {
            let so_far = lock(&seen).clone();
            let plain = as_a_reader_sees(&so_far);
            if plain.contains(text) {
                return plain;
            }
            if running_since.is_none() && !plain.trim().is_empty() {
                running_since = Some(Instant::now());
            }
            match running_since {
                None => assert!(
                    watched.elapsed() < TO_START,
                    "the program printed nothing in {TO_START:?} (raw: {so_far:?})"
                ),
                Some(since) => assert!(
                    since.elapsed() < ONCE_RUNNING,
                    "{text:?} never arrived, only {plain:?} (raw: {so_far:?})"
                ),
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Terminal output as the words on the screen: escape sequences dropped, and the breaks
    /// a terminal inserts — a wrap, a carriage return — read as the single space that was
    /// between the words before it laid them out.
    ///
    /// **Every escape, not only CSI** (charter-app#169). This used to drop an `ESC` and then
    /// step over the sequence only when the next byte was `[`; every other escape lost its
    /// `ESC` and kept the rest as TEXT. So `ESC 7` — DECSC, save cursor, two bytes — put a
    /// literal `7` into what this function claims a reader sees, and a redraw landing between
    /// the stand-in's two writes of its argv turned
    /// `--resume <id> --name ide.7` into `--resume <id> 7 --name ide.7` and failed a chats
    /// test that had nothing wrong with it. Seen once on CI, green on a rerun and 8/8
    /// locally on the same commit, which is what an assertion about a race looks like.
    ///
    /// A helper that reports a failure that did not happen is worse than no helper, so this
    /// consumes the escapes a terminal actually emits rather than the one byte that was
    /// caught: [`eat_escape`] has the shapes and why each is here.
    fn as_a_reader_sees(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                eat_escape(&mut chars);
                continue;
            }
            out.push(if c == '\r' || c == '\n' { ' ' } else { c });
        }
        // A wrap becomes one space, and so does a run of them, so a command line reads the
        // way it was written however the pane laid it out.
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Step over the rest of one escape sequence, the `ESC` itself already taken.
    ///
    /// The four shapes ECMA-48 gives an escape, because a test helper that knows only one of
    /// them is a helper that invents characters (charter-app#169):
    ///
    /// - **CSI** (`ESC [`) — parameter and intermediate bytes, then one final byte. The final
    ///   byte is `0x40..=0x7e` rather than "a letter or `~`": `ESC [ 2 q` (the cursor shape
    ///   charter's own engine emits, and the sequence standing next to the `ESC 7` in the
    ///   failing run) ends on `q`, but `ESC [ 0 c` and the `}`-final forms do not, and a
    ///   final byte this stopped short of would spill parameters into the text.
    /// - **string sequences** — OSC (`ESC ]`, a window title), DCS, SOS, PM, APC — run to a
    ///   string terminator: `ESC \`, or BEL, which every terminal accepts for OSC and which
    ///   is what `xterm.js` and this engine emit.
    /// - **nF** — an intermediate byte (`0x20..=0x2f`) then a final one: `ESC ( B` puts
    ///   US-ASCII into G0, which a harness clearing the screen emits, and `ESC # 8` is DECALN.
    /// - **everything else is the whole sequence**: `ESC 7`/`ESC 8` (save and restore cursor),
    ///   `ESC =`/`ESC >` (keypad mode), `ESC M` (reverse index), `ESC c` (full reset). These
    ///   are the ones that were leaving a character behind.
    fn eat_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if matches!(c, '\u{40}'..='\u{7e}') {
                        break;
                    }
                }
            }
            Some(']' | 'P' | 'X' | '^' | '_') => {
                let mut closed = None;
                for c in chars.by_ref() {
                    if c == '\u{7}' || c == '\u{1b}' {
                        closed = Some(c);
                        break;
                    }
                }
                // `ESC \` is the terminator; the `ESC` is consumed above and the `\` here.
                if closed == Some('\u{1b}') {
                    chars.next_if_eq(&'\\');
                }
            }
            Some('\u{20}'..='\u{2f}') => {
                for c in chars.by_ref() {
                    if !matches!(c, '\u{20}'..='\u{2f}') {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// charter-app#169. `ESC 7` is a saved cursor, and a reader sees nothing of it.
    #[test]
    fn a_redraw_between_two_writes_adds_no_character_a_reader_could_see() {
        // The bytes CI captured (run 35758178617, job 106849600023, PR #168's head), with the
        // id shortened: a redraw landed between the stand-in's `argv:` and its arguments.
        let raw = "argv: --resume 1111\u{1b}[K\r\n\u{1b}[1;1H\u{1b}7\u{1b}[2 q\u{1b}[1;52H \
                   --name ide.7\r\n";

        assert_eq!(as_a_reader_sees(raw), "argv: --resume 1111 --name ide.7");
    }

    /// Each shape [`eat_escape`] knows, consumed whole — `a` and `b` stay adjacent.
    #[test]
    fn every_escape_a_terminal_emits_is_consumed_whole() {
        for (raw, what) in [
            ("a\u{1b}7b", "ESC 7, save cursor"),
            ("a\u{1b}8b", "ESC 8, restore cursor"),
            ("a\u{1b}=b", "ESC =, application keypad"),
            ("a\u{1b}>b", "ESC >, normal keypad"),
            ("a\u{1b}Mb", "ESC M, reverse index"),
            ("a\u{1b}cb", "ESC c, full reset"),
            ("a\u{1b}(Bb", "ESC ( B, US-ASCII into G0"),
            ("a\u{1b}#8b", "ESC # 8, DECALN"),
            ("a\u{1b}]0;a window title\u{7}b", "OSC closed by BEL"),
            ("a\u{1b}]0;a window title\u{1b}\\b", "OSC closed by ST"),
            ("a\u{1b}[1;1Hb", "CSI, cursor home"),
            ("a\u{1b}[?25lb", "CSI with a private parameter"),
            ("a\u{1b}[2 qb", "CSI with an intermediate byte"),
            ("a\u{1b}[0mb", "CSI, reset"),
        ] {
            assert_eq!(as_a_reader_sees(raw), "ab", "{what} left something behind");
        }
    }

    /// Chats that write every record they make into `wrote`, newest last.
    fn recorded() -> (Chats, std::sync::Arc<Mutex<Vec<Record>>>) {
        let wrote = std::sync::Arc::new(Mutex::new(Vec::new()));
        let keep = std::sync::Arc::clone(&wrote);
        let chats = Chats::recorded_by(Box::new(move |record| lock(&keep).push(record.clone())));
        (chats, wrote)
    }

    fn a_view(key: &str) -> purlis_core::reopen::View {
        purlis_core::reopen::View {
            from: None,
            view: "persona".into(),
            key: key.into(),
            title: key.into(),
            workspace: None,
            at: 0,
            active: false,
            pinned: false,
            split: None,
        }
    }

    #[test]
    fn the_view_tabs_the_window_holds_are_written_into_the_record_beside_the_chats() {
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.hold_views(vec![a_view("steward")]);

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(
            last.chats.len(),
            1,
            "the chats went missing from the record"
        );
        assert_eq!(last.views, vec![a_view("steward")]);
    }

    fn a_focus() -> Focus {
        Focus {
            workspace: "alpha".into(),
            repo: "svc".into(),
            piece: Some("fix-login".into()),
        }
    }

    #[test]
    fn the_branch_the_window_focused_is_written_into_the_record_once() {
        // FM-5: the cockpit's branch is remembered with the window's views.
        let (chats, wrote) = recorded();

        chats.hold_focus(Some(a_focus()));
        let so_far = lock(&wrote).len();
        chats.hold_focus(Some(a_focus()));

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(last.focus, Some(a_focus()));
        assert_eq!(
            lock(&wrote).len(),
            so_far,
            "the same focus was written twice"
        );
        chats.hold_focus(None);
        assert_eq!(lock(&wrote).last().cloned().unwrap().focus, None);
    }

    #[test]
    fn a_record_s_focus_is_held_for_the_window_when_it_is_put_back() {
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();

        chats.put_back(
            &Record {
                focus: Some(a_focus()),
                ..Default::default()
            },
            dir.path(),
            SIZE,
        );

        assert_eq!(chats.focus(), Some(a_focus()));
        assert!(lock(&wrote).is_empty(), "putting a record back wrote it");
    }

    #[test]
    fn saying_the_same_view_tabs_again_writes_nothing() {
        // The window says what its view tabs are after every change to its tabs, and most of
        // those changes are to chats.
        let (chats, wrote) = recorded();
        chats.hold_views(vec![a_view("steward")]);
        let so_far = lock(&wrote).len();

        chats.hold_views(vec![a_view("steward")]);

        assert_eq!(lock(&wrote).len(), so_far);
    }

    #[test]
    fn a_record_s_view_tabs_are_held_for_the_window_and_not_written_back_while_it_is_put_back() {
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();

        chats.put_back(
            &Record {
                views: vec![a_view("steward")],
                ..Default::default()
            },
            dir.path(),
            SIZE,
        );

        assert_eq!(chats.views(), vec![a_view("steward")]);
        assert!(lock(&wrote).is_empty(), "putting a record back wrote it");
    }

    /// The names the record lists its chats under, in the order it lists them.
    fn names_in(record: &Record) -> Vec<String> {
        record.chats.iter().map(|chat| chat.name.clone()).collect()
    }

    #[test]
    fn the_record_lists_the_chats_in_the_order_the_window_arranged_them() {
        // SI-6: the operator drags a tab, and the strip's order is what comes back at the next
        // launch — not the order the chats happened to be numbered in.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        let a = chats.start(&chat(&claude, "a", None), SIZE).unwrap();
        let b = chats.start(&chat(&claude, "b", None), SIZE).unwrap();
        let c = chats.start(&chat(&claude, "c", None), SIZE).unwrap();

        chats.hold_order(vec![c, a, b]);

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(names_in(&last), ["c", "a", "b"]);
        let open: Vec<String> = chats.open_now().into_iter().map(|one| one.name).collect();
        assert_eq!(
            open,
            ["c", "a", "b"],
            "a reloaded window would draw another order"
        );
    }

    #[test]
    fn saying_the_same_chat_order_again_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        let a = chats.start(&chat(&claude, "a", None), SIZE).unwrap();
        let b = chats.start(&chat(&claude, "b", None), SIZE).unwrap();
        chats.hold_order(vec![b, a]);
        let so_far = lock(&wrote).len();

        chats.hold_order(vec![b, a]);

        assert_eq!(lock(&wrote).len(), so_far);
    }

    #[test]
    fn saying_the_order_the_record_already_has_writes_nothing() {
        // The window says the order after every change to its tabs, opening a chat included,
        // and a chat it has just opened is already last in the record.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        let a = chats.start(&chat(&claude, "a", None), SIZE).unwrap();
        let b = chats.start(&chat(&claude, "b", None), SIZE).unwrap();
        let so_far = lock(&wrote).len();

        chats.hold_order(vec![a, b]);

        assert_eq!(lock(&wrote).len(), so_far);
    }

    #[test]
    fn a_chat_the_window_has_not_placed_yet_comes_after_the_ones_it_has() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        let a = chats.start(&chat(&claude, "a", None), SIZE).unwrap();
        let b = chats.start(&chat(&claude, "b", None), SIZE).unwrap();
        chats.hold_order(vec![b, a]);

        chats.start(&chat(&claude, "c", None), SIZE).unwrap();

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(names_in(&last), ["b", "a", "c"]);
    }

    // --- the sandbox (ADR 0067): a chat on no profile goes through the same decision ------- //

    #[test]
    fn a_sandboxed_opencode_chat_runs_inside_charters_wrap_and_its_proxy_lives_as_long_as_it() {
        // opencode has no sandbox of its own, so the whole harness runs under the profile
        // charter writes, reaching the network through charter's proxy alone (ADR 0067 §2).
        let plane = a_sandboxed_plane();
        let plugin = plane.path().join("plugin");
        let shim = purlis_core::opencode::shim_in(&plugin);
        std::fs::create_dir_all(shim.parent().expect("a parent")).expect("the bundle");
        std::fs::write(&shim, "export default {}\n").expect("the shim");
        let socket = plane.path().join(".charter/app/hooks.sock");
        let host = Pretend::default();
        host.reporting_on(socket.clone());
        let mut chats = Chats::on_host(Box::new(|_| {}), Box::new(host.clone()));
        chats.arming_with(crate::Shipped {
            binary: Some(plane.path().join("charter")),
            plugin: Some(plugin),
            shims: None,
            git_hooks: None,
        });
        // A profile that names its own proxy and temp directory: the wrap's win, or the chat's
        // traffic and temp files would go where the profile says.
        let ready = purlis_core::start::Ready {
            env: vec![
                ("ZZ_KEPT".to_owned(), "yes".to_owned()),
                ("HTTPS_PROXY".to_owned(), "http://zz.example:1".to_owned()),
                ("NO_PROXY".to_owned(), "*".to_owned()),
                ("TMPDIR".to_owned(), "/zz".to_owned()),
            ],
            ..ready_under(
                Harness::Opencode,
                a_sandbox_for(Harness::Opencode, plane.path()),
            )
        };
        let chat = Chat {
            cwd: Some(plane.path().to_path_buf()),
            ..chat("/bin/sh", "c", None)
        };

        let session = chats.start_ready(&chat, &ready, SIZE).expect("starts");

        let opening = host.openings().pop().expect("opened");
        assert_eq!(opening.program.as_deref(), Some("/usr/bin/sandbox-exec"));
        assert_eq!(opening.args[0], "-p");
        assert!(
            opening.args[1].contains("(remote unix-socket (path-literal"),
            "{}",
            opening.args[1]
        );
        assert_eq!(opening.args[2..], ["/bin/sh", "-c", "sleep 30"]);
        let named = |wanted: &str| -> Vec<String> {
            opening
                .env
                .iter()
                .filter(|(key, _)| key == wanted)
                .map(|(_, value)| value.clone())
                .collect()
        };
        assert_eq!(named("NO_PROXY"), [""]);
        assert_eq!(named("ZZ_KEPT"), ["yes"]);
        // The word the chat's own processes read for "this chat was given a sandbox" (#1345).
        assert_eq!(named(purlis_core::hookwire::SANDBOXED_ENV), ["1"]);
        let tmp = named("TMPDIR");
        assert!(tmp.len() == 1 && tmp[0] != "/zz", "{tmp:?}");
        let proxy = match named("HTTPS_PROXY").as_slice() {
            [one] => one.clone(),
            many => panic!("pointed at {many:?}"),
        };
        let port: u16 = proxy
            .rsplit(':')
            .next()
            .and_then(|port| port.parse().ok())
            .expect("a port");
        assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());

        chats.close(session).expect("closed");
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(
            std::net::TcpStream::connect(("127.0.0.1", port)).is_err(),
            "the proxy outlived its chat"
        );
    }

    #[test]
    fn a_chat_started_without_a_sandbox_is_never_told_it_has_one() {
        // #1345: what a chat says about "this chat's sandbox" is read from this word, so a
        // chat the app started unsandboxed must not carry it, even in a sandboxed project.
        let plane = a_sandboxed_plane();
        let host = Pretend::default();
        let chats = Chats::on_host(Box::new(|_| {}), Box::new(host.clone()));
        let ready = purlis_core::start::Ready {
            sandbox: None,
            env: vec![(
                purlis_core::hookwire::SANDBOXED_ENV.to_owned(),
                "1".to_owned(),
            )],
            ..ready_under(Harness::ClaudeCode, a_claude_sandbox(plane.path()))
        };
        let chat = Chat {
            cwd: Some(plane.path().to_path_buf()),
            ..chat("/bin/sh", "c", None)
        };

        let session = chats.start_ready(&chat, &ready, SIZE).expect("starts");

        let opening = host.openings().pop().expect("opened");
        assert!(
            !opening
                .env
                .iter()
                .any(|(key, _)| key == purlis_core::hookwire::SANDBOXED_ENV),
            "{:?}",
            opening.env
        );
        chats.close(session).expect("closed");
    }

    /// A plane that turned the sandbox on.
    fn a_sandboxed_plane() -> tempfile::TempDir {
        let plane = tempfile::tempdir().expect("a plane");
        std::fs::write(
            plane.path().join(purlis_core::plane::MANIFEST),
            "[sandbox]\nmode = \"on\"\n",
        )
        .expect("charter.toml");
        plane
    }

    /// A chat on no profile whose program is `program`, standing in `plane`.
    fn a_chat_in(plane: &std::path::Path, program: &str) -> Chat {
        Chat {
            cwd: Some(plane.to_path_buf()),
            ..chat(program, "off-profile", None)
        }
    }

    #[test]
    fn a_harness_opened_on_no_profile_in_a_sandboxed_plane_is_refused_not_run_unconfined() {
        // `open_session` with a harness as its program: no profile, and still a harness.
        let plane = a_sandboxed_plane();
        let chats = Chats::new();

        let refused = chats
            .start(&a_chat_in(plane.path(), "/nowhere/opencode"), SIZE)
            .expect_err("not started");

        // Refused for one reason or another on every system — no compiler here, no wrap there,
        // no shipped binary to arm it with — and never started unconfined.
        assert!(
            refused.starts_with("this plane runs every chat sandboxed"),
            "{refused}"
        );
        assert!(chats.in_order().is_empty(), "a chat was opened");
    }

    #[test]
    fn a_recorded_harness_chat_on_no_profile_is_not_put_back_unconfined() {
        let plane = a_sandboxed_plane();
        // A program that would run, so only the sandbox decision can keep it from starting.
        let opencode = stand_in::program(plane.path(), "opencode", "#!/bin/sh\nsleep 600\n");
        let chats = Chats::new();

        let open = chats.put_back(
            &Record {
                chats: vec![a_chat_in(plane.path(), &opencode.display().to_string())],
                ..Default::default()
            },
            plane.path(),
            SIZE,
        );

        assert!(open.is_empty(), "it was put back");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_harness_on_no_profile_whose_program_a_chat_could_write_is_refused() {
        // Ruling V87g: the system temp folders are writable to a chat, and so is the plane.
        let plane = a_sandboxed_plane();
        let elsewhere = tempfile::tempdir().expect("a temp folder");
        let opencode = stand_in::program(elsewhere.path(), "opencode", "#!/bin/sh\nsleep 600\n");
        let chats = Chats::new();

        let refused = chats
            .start(
                &a_chat_in(plane.path(), &opencode.display().to_string()),
                SIZE,
            )
            .expect_err("not started");

        assert!(
            refused.contains("the program lives where this chat can write"),
            "{refused}"
        );
        assert!(chats.in_order().is_empty(), "a chat was opened");
    }

    #[cfg(unix)]
    #[test]
    fn a_harness_on_no_profile_under_a_charter_toml_that_is_not_a_file_is_refused() {
        // The plane's walk takes only a regular file for a plane: a FIFO, or a link, there
        // would otherwise read as no plane at all and start the chat unsandboxed.
        for kind in ["fifo", "dangling"] {
            let plane = tempfile::tempdir().expect("a plane");
            let marker = plane.path().join(purlis_core::plane::MANIFEST);
            if kind == "fifo" {
                let made = purlis_core::forklock::status(
                    std::process::Command::new("mkfifo").arg(&marker),
                )
                .expect("mkfifo runs");
                assert!(made.success());
            } else {
                std::os::unix::fs::symlink("gone", &marker).expect("a link");
            }
            let below = plane.path().join("workspaces/w");
            std::fs::create_dir_all(&below).expect("a workspace");
            let chats = Chats::new();
            let refused = chats
                .start(
                    &Chat {
                        cwd: Some(below),
                        ..chat("/nowhere/opencode", "off-profile", None)
                    },
                    SIZE,
                )
                .expect_err("not started");
            assert!(
                refused.contains("cannot be read as TOML"),
                "{kind}: {refused}"
            );
            assert!(chats.in_order().is_empty(), "{kind}: a chat was opened");
        }
    }

    #[test]
    fn a_shell_in_a_sandboxed_plane_is_still_the_operators_own() {
        let plane = a_sandboxed_plane();
        let chats = Chats::new();

        let session = chats
            .start(&a_chat_in(plane.path(), "/bin/sh"), SIZE)
            .expect("a shell starts");

        let _ = chats.close(session);
    }

    /// The sandbox the core compiles for a `harness` chat in `plane`, on a machine that has
    /// every backend program — so the answer does not depend on the machine the test runs on.
    fn a_sandbox_for(harness: Harness, plane: &std::path::Path) -> purlis_core::sandbox::Applied {
        let machine = purlis_core::sandbox::Machine {
            env: purlis_core::secrets::Env::of(&[]),
            home: None,
            // Where every harness has a sandbox charter compiles.
            os: purlis_core::sandbox::Os::MacOs,
        };
        purlis_core::sandbox::for_start(harness, plane, &machine, &|_| true)
            .expect("compiles")
            .expect("sandboxed")
    }

    fn a_claude_sandbox(plane: &std::path::Path) -> purlis_core::sandbox::Applied {
        a_sandbox_for(Harness::ClaudeCode, plane)
    }

    fn ready_under(
        harness: Harness,
        sandbox: purlis_core::sandbox::Applied,
    ) -> purlis_core::start::Ready {
        purlis_core::start::Ready {
            program: "/bin/sh".to_owned(),
            command: vec!["-c".to_owned(), "sleep 30".to_owned()],
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            harness: Some(harness),
            session: None,
            how: purlis_core::reopen::Reopened::Fresh(Fresh::NoConversationRecorded),
            plugins: std::collections::BTreeMap::new(),
            sandbox: Some(sandbox),
            unsandboxed: None,
            notices: Vec::new(),
            agents_md: Vec::new(),
        }
    }

    #[test]
    fn a_sandbox_the_app_is_not_armed_to_hand_over_refuses_the_chat() {
        // No plugin shipped: Claude Code is armed with nothing, so the sandbox would be
        // dropped. The chat is refused instead.
        let plane = a_sandboxed_plane();
        let mut chats = Chats::new();
        chats.arming_with(crate::Shipped {
            binary: Some(plane.path().join("charter")),
            plugin: None,
            shims: None,
            git_hooks: None,
        });
        let ready = ready_under(Harness::ClaudeCode, a_claude_sandbox(plane.path()));

        let refused = chats
            .start_ready(&chat("/bin/sh", "c", None), &ready, SIZE)
            .expect_err("not started");

        assert!(refused.contains("purlis's plugin"), "{refused}");
    }

    #[test]
    fn a_sandbox_compiled_for_one_harness_never_reaches_another() {
        let plane = a_sandboxed_plane();
        let mut chats = Chats::new();
        chats.arming_with(crate::Shipped {
            binary: Some(plane.path().join("charter")),
            plugin: Some(plane.path().join("plugin")),
            shims: None,
            git_hooks: None,
        });
        let ready = ready_under(Harness::Codex, a_claude_sandbox(plane.path()));

        let refused = chats
            .start_ready(&chat("/bin/sh", "c", None), &ready, SIZE)
            .expect_err("not started");

        assert!(refused.contains("cannot hand the sandbox"), "{refused}");
    }

    #[test]
    fn a_codex_chat_on_no_profile_in_a_sandboxed_plane_is_never_started_unsandboxed() {
        // #1123: charter wraps Codex where it can (macOS); a program that is not there, or a
        // system charter cannot wrap it on, refuses the chat rather than starting it without.
        let plane = a_sandboxed_plane();
        let chats = Chats::new();

        let refused = chats
            .start(&a_chat_in(plane.path(), "/nowhere/codex"), SIZE)
            .expect_err("not started");

        assert!(!refused.contains("#1123"), "{refused}");
        assert!(refused.contains("started"), "{refused}");
        assert!(chats.in_order().is_empty(), "a chat was opened");
    }

    #[test]
    fn a_record_is_put_back_in_its_own_order_and_not_by_chat_number() {
        // The record lists the chats in the order the strip drew them, and a chat keeps its
        // number across a launch (charter-app#90) — so number order is not strip order.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back_here(
            &Record {
                chats: vec![
                    Chat {
                        number: Some(5),
                        ..chat(&claude, "dragged first", None)
                    },
                    Chat {
                        number: Some(2),
                        ..chat(&claude, "opened first", None)
                    },
                ],
                ..Default::default()
            },
            SIZE,
        );

        let names: Vec<&str> = open.iter().map(|one| one.name.as_str()).collect();
        assert_eq!(names, ["dragged first", "opened first"]);
        assert_eq!(names_in(&chats.record()), ["dragged first", "opened first"]);
    }

    #[test]
    fn opening_a_chat_writes_the_record_without_waiting_for_a_quit() {
        // An app that is killed, or crashes, runs no exit handler. Everything open would be
        // lost if the record were only written on the way out.
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();

        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(last.chats.len(), 1);
        assert_eq!(last.chats[0].name, "ide.7");
    }

    #[test]
    fn closing_a_chat_writes_the_record_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();
        let going = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.close(going).expect("it closes");

        assert_eq!(lock(&wrote).last().expect("a record").chats, vec![]);
    }

    #[test]
    fn bringing_another_chat_to_the_front_writes_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        chats.start(&chat(&claude, "ide.7", None), SIZE).unwrap();
        let front = chats.start(&chat(&claude, "ide.8", None), SIZE).unwrap();

        chats.bring_to_front(Some(front));

        let last = lock(&wrote).last().cloned().expect("a record");
        let active: Vec<&str> = last
            .chats
            .iter()
            .filter(|c| c.active)
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(active, vec!["ide.8"]);
    }

    #[test]
    fn bringing_the_same_chat_to_the_front_again_writes_nothing() {
        // Every click on the tab already in front would otherwise be a write.
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();
        let only = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();
        chats.bring_to_front(Some(only));
        let so_far = lock(&wrote).len();

        chats.bring_to_front(Some(only));

        assert_eq!(lock(&wrote).len(), so_far);
    }

    #[test]
    fn putting_a_record_back_writes_it_once_when_every_chat_is_back() {
        // Fifty chats coming back must not be fifty writes at the one moment cold start is
        // measured. One write, though, there has to be: each chat came back in a run begun at
        // this launch, and a chat recorded before ids with one minted for it, which an app
        // that crashed before its next write would mint again (#856 review F1).
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();

        chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: (0..5)
                    .map(|n| chat(&claude, &format!("ide.{n}"), None))
                    .collect(),
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        let written = lock(&wrote).clone();
        assert_eq!(written.len(), 1, "{written:#?}");
        assert_eq!(written[0].chats.len(), 5, "every chat, in the one write");
        assert!(
            written[0]
                .chats
                .iter()
                .all(|one| one.identity.id.is_some() && one.identity.run.is_some()),
            "{written:#?}"
        );
    }

    #[test]
    fn a_chat_that_was_started_is_one_the_quit_would_record() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();

        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .expect("the chat starts");

        let record = chats.record();
        assert_eq!(record.chats.len(), 1);
        assert_eq!(record.chats[0].name, "ide.7");
        assert_eq!(chats.open_now()[0].session, session);
        assert_eq!(chats.open_now()[0].harness, Some(Harness::ClaudeCode));
    }

    #[test]
    fn a_chats_harness_is_answered_by_its_session_number_and_a_shells_is_none() {
        // What a pane asks as it opens its view (SI-4): Shift+Enter is the harness's newline,
        // and a shell keeps the terminal's own Enter.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let claude = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .expect("the chat starts");
        let shell = chats
            .start(&chat("/bin/sh", "a shell", None), SIZE)
            .expect("the shell starts");

        assert_eq!(chats.harness(claude), Some(Harness::ClaudeCode));
        assert_eq!(chats.harness(shell), None);
        assert_eq!(
            chats.harness(claude + shell + 1),
            None,
            "a chat that is not open"
        );
    }

    #[test]
    fn the_record_holds_the_conversation_id_the_app_chose_for_a_new_claude_chat() {
        // The whole point of the record: a chat started fresh today is resumable tomorrow.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        let recorded = chats.record().chats[0].resume.clone();

        assert!(
            recorded.is_some(),
            "the chat was recorded with no conversation"
        );
    }

    #[test]
    fn the_id_in_the_record_is_the_one_the_harness_was_actually_given() {
        // Recording an id the harness never saw would give a resume that always failed.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        // The id charter chose is known before the harness has finished printing it, so the
        // wait is for that exact id rather than for the line it will appear on.
        let recorded = chats.record().chats[0]
            .resume
            .clone()
            .expect("the chat has a conversation");
        let printed = until_printed(&chats, session, recorded.as_str());

        assert!(
            printed.contains(recorded.as_str()),
            "the record says {recorded}, but claude was given {printed:?}"
        );
    }

    #[test]
    fn a_chat_that_closed_is_not_in_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let going = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.8", None), SIZE)
            .unwrap();

        chats.close(going).expect("it closes");

        let names: Vec<String> = chats
            .record()
            .chats
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(names, vec!["ide.8"]);
    }

    #[test]
    fn the_chat_in_front_is_the_one_the_record_marks_active() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();
        let front = chats
            .start(&chat(&a_claude(dir.path()), "ide.8", None), SIZE)
            .unwrap();

        chats.bring_to_front(Some(front));

        let active: Vec<String> = chats
            .record()
            .chats
            .iter()
            .filter(|c| c.active)
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(active, vec!["ide.8"]);
    }

    #[test]
    fn a_chat_that_closed_costs_nothing_to_remember() {
        // An app left running all day closes chats all day. Each one that stayed remembered
        // would be a little more memory that never comes back — invisible without this.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let going = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.close(going).expect("it closes");

        assert_eq!(chats.remembered(), 0);
    }

    #[test]
    fn the_chat_that_was_in_front_comes_back_in_front() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();
        let was_in_front = Chat {
            active: true,
            profile: None,
            persona: None,
            ..chat(&claude, "ide.8", None)
        };

        chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![chat(&claude, "ide.7", None), was_in_front],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        let active: Vec<String> = chats
            .record()
            .chats
            .iter()
            .filter(|c| c.active)
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(active, vec!["ide.8"]);
    }

    #[test]
    fn a_record_is_put_back_as_one_session_for_each_chat_it_holds() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![
                    chat(&claude, "ide.7", Some(ID)),
                    chat(&claude, "ide.8", None),
                ],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        assert_eq!(open.len(), 2);
        assert_eq!(chats.sessions().running().len(), 2);
        assert_eq!(open[0].name, "ide.7");
        assert_eq!(open[1].name, "ide.8");
    }

    #[test]
    fn a_chat_put_back_with_a_conversation_is_resumed_by_it() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();

        let open = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![chat(&a_claude(dir.path()), "ide.7", Some(ID))],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        assert_eq!(open[0].how, Reopened::Resumed(SessionId::new(ID).unwrap()));
        let want = format!("--resume {ID} --name ide.7");
        let printed = until_printed(&chats, open[0].session, &want);
        assert!(
            printed.contains(&want),
            "claude was not asked to resume: {printed:?}"
        );
    }

    #[test]
    fn the_chat_that_was_in_front_is_the_one_the_window_is_told_to_show() {
        // The record holds which chat was in front; without this the window would put every
        // chat back and then show whichever one it happened to draw last.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![
                    chat(&claude, "ide.7", None),
                    Chat {
                        active: true,
                        profile: None,
                        persona: None,
                        ..chat(&claude, "ide.8", None)
                    },
                ],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        let in_front: Vec<&str> = open
            .iter()
            .filter(|one| one.in_front)
            .map(|one| one.name.as_str())
            .collect();
        assert_eq!(in_front, vec!["ide.8"]);
    }

    #[test]
    fn a_chat_put_back_with_no_conversation_says_so_and_starts_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();

        let open = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![chat(&a_claude(dir.path()), "ide.7", None)],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        assert_eq!(open[0].how, Reopened::Fresh(Fresh::NoConversationRecorded));
    }

    #[test]
    fn the_record_names_the_process_each_chat_runs_as_now_and_not_the_one_it_was_put_back_with() {
        // V82 (#1018): `commit-msg` stamps a commit only below this process, so a pid carried
        // over from the last launch would name a process that is gone.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();

        let open = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![Chat {
                    pid: Some(4_000_000),
                    ..chat(&a_claude(dir.path()), "ide.7", Some(ID))
                }],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        let running = chats.sessions.process_id(open[0].session);
        assert!(running.is_some());
        assert_eq!(chats.record().chats[0].pid, running);
    }

    #[test]
    fn the_last_record_is_written_after_every_other_and_names_no_pid() {
        // R2-1: a write already on its way when the quit begins must not land after the quit's
        // own and put live pids back on disk.
        let dir = tempfile::tempdir().unwrap();
        let written: std::sync::Arc<Mutex<Vec<Record>>> = std::sync::Arc::default();
        let chats = Chats::recorded_by(Box::new({
            let written = std::sync::Arc::clone(&written);
            move |record| lock(&written).push(record.clone())
        }));
        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .expect("the chat starts");
        lock(&written).clear();

        std::thread::scope(|scope| {
            chats.write_last(|record| {
                // A program's end heard, on its own thread, while the quit's record is being
                // written: it must not land after it.
                scope.spawn(|| chats.a_program_ended());
                std::thread::sleep(std::time::Duration::from_millis(50));
                lock(&written).push(record.clone());
            });
        });
        chats.a_program_ended();
        chats.end_all();

        let written = lock(&written);
        assert_eq!(written.len(), 1, "{written:?}");
        assert_eq!(
            written[0].chats.len(),
            1,
            "the chat is kept for the next launch"
        );
        assert_eq!(written[0].chats[0].pid, None);
    }

    #[test]
    fn a_claude_chat_reopened_after_its_workspace_was_renamed_starts_fresh_and_says_so_once() {
        // charter#367, D10: Claude Code keeps the conversation under the old folder, so the
        // rename dropped it from the record. The reopen starts a new conversation instead of a
        // `--resume` that would fail, says why, and records the chat as an ordinary one.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("plane");
        std::fs::create_dir_all(root.join("workspaces/beta")).unwrap();
        let mut record = Record {
            views: Vec::new(),
            chats: vec![Chat {
                cwd: Some(root.join("workspaces/alpha")),
                ..chat(&a_claude(dir.path()), "ide.7", Some(ID))
            }],
            dealt: 0,
            relaunch_after_update: false,
            clone_seat: None,
            focus: None,
        };
        // What `charter workspace rename alpha beta` does to the record.
        assert!(
            purlis_core::wscmd::rename::Move::in_plane(&root, "alpha", "beta").record(&mut record)
        );
        let chats = Chats::new();

        let open = chats.put_back_here(&record, SIZE);

        assert_eq!(open[0].how, Reopened::Fresh(Fresh::WorkspaceRenamed));
        let printed = until_printed(&chats, open[0].session, "--session-id");
        assert!(!printed.contains("--resume"), "{printed:?}");
        let recorded = &chats.record().chats[0];
        assert_eq!(
            recorded.renamed_from, None,
            "it would say so again next time"
        );
        assert!(
            recorded.resume.is_some(),
            "the new conversation is not recorded"
        );
        assert_ne!(recorded.resume, Some(SessionId::new(ID).unwrap()));
    }

    #[test]
    fn a_chat_that_could_not_be_started_stays_in_the_record_for_the_next_launch() {
        // Otherwise a workspace directory that is moved, or a harness that is being
        // reinstalled, silently deletes the chat: it fails to start once, the record is
        // written without it, and by the launch after that there is no trace it existed.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();

        chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![
                    chat("/definitely/not/a/program", "ide.7", Some(ID)),
                    chat(&claude, "ide.8", None),
                ],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        let names: Vec<String> = chats
            .record()
            .chats
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(names, vec!["ide.7", "ide.8"]);
        let written: Vec<Vec<String>> = lock(&wrote)
            .iter()
            .map(|record| record.chats.iter().map(|c| c.name.clone()).collect())
            .collect();
        assert_eq!(
            written,
            vec![vec!["ide.7".to_owned(), "ide.8".to_owned()]],
            "the one write a put-back makes keeps the chat that did not start"
        );
    }

    #[test]
    fn a_record_cannot_ask_a_launch_to_start_an_unbounded_number_of_programs() {
        // At a bound of four, not the app's two hundred: each start opens a terminal, and two
        // test binaries at two hundred each ran macOS out of them (511), failing every other
        // test that opened a chat (#1139). The bound itself is the next test's.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new().starting_at_most(4);

        let open = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: (0..4 + 3)
                    .map(|n| chat(&claude, &format!("ide.{n}"), None))
                    .collect(),
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        assert_eq!(open.len(), 4);
        // And the ones it would not start are kept, not thrown away, saying why.
        let kept = chats.would_not_start();
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[0].why, "more than 4 chats were recorded");
        chats.end_all();
    }

    #[test]
    fn the_app_starts_at_most_two_hundred_chats_from_a_record() {
        // The product's scale is fifty; the backstop sits far above it.
        assert_eq!(Chats::new().most_at_once, 200);
    }

    #[test]
    fn a_chat_that_could_not_be_started_is_named_with_the_reason() {
        // The operator is told, rather than finding a tab quietly missing. Nothing here
        // starts, so there is no stand-in harness to put anywhere.
        let (chats, _) = recorded();

        chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![chat("/definitely/not/a/program", "ide.7", Some(ID))],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        let trouble = chats.would_not_start();
        assert_eq!(trouble.len(), 1);
        assert_eq!(trouble[0].name, "ide.7");
        assert!(!trouble[0].why.is_empty(), "no reason was kept");
    }

    #[test]
    fn a_chat_that_could_not_be_started_is_still_recorded_after_a_later_change() {
        // The record is written again as soon as anything changes; the chat that could not
        // start has to survive that write too, not just the launch.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![chat("/definitely/not/a/program", "ide.7", Some(ID))],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        chats.start(&chat(&claude, "ide.9", None), SIZE).unwrap();

        let last = lock(&wrote).last().cloned().expect("a record was written");
        let names: Vec<String> = last.chats.iter().map(|c| c.name.clone()).collect();
        assert_eq!(names, vec!["ide.7", "ide.9"]);
    }

    /// A record holding one chat, `ide.7`, that runs `program` and resumes [`ID`].
    fn one_recorded(program: &str) -> Record {
        Record {
            views: Vec::new(),
            chats: vec![chat(program, "ide.7", Some(ID))],
            dealt: 0,
            relaunch_after_update: false,
            clone_seat: None,
            focus: None,
        }
    }

    /// The id of the one chat a record put back that is waiting to start.
    fn waiting_id(chats: &Chats) -> String {
        let waiting = chats.would_not_start();
        assert_eq!(waiting.len(), 1, "one chat waiting");
        assert!(!waiting[0].id.is_empty(), "a waiting chat has an id");
        waiting[0].id.clone()
    }

    /// How many times each record written since `from` holds the chat with id `id`.
    fn times_recorded(wrote: &Mutex<Vec<Record>>, from: usize, id: &str) -> Vec<usize> {
        lock(wrote)[from..]
            .iter()
            .map(|record| {
                record
                    .chats
                    .iter()
                    .filter(|one| one.identity.id.as_deref() == Some(id))
                    .count()
            })
            .collect()
    }

    const HERE: &str = "/nonexistent-plane";

    #[test]
    fn a_chat_that_did_not_start_is_started_by_retry_once_what_it_needs_is_back() {
        // NO-3: Retry now, after the harness was reinstalled. The chat is the recorded one,
        // under its id — and every record written on the way holds it exactly once: never
        // twice (waiting and running), never not at all.
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("claude").display().to_string();
        let (chats, wrote) = recorded();
        chats.put_back_here(&one_recorded(&program), SIZE);
        let id = waiting_id(&chats);
        let from = lock(&wrote).len();

        assert_eq!(a_claude(dir.path()), program);
        let session = chats
            .retry(&id, std::path::Path::new(HERE), SIZE)
            .expect("it starts now");

        assert!(chats.would_not_start().is_empty());
        let open: Vec<(u32, String)> = chats
            .open_now()
            .into_iter()
            .map(|one| (one.session, one.name))
            .collect();
        assert_eq!(open, vec![(session, "ide.7".to_owned())]);
        let times = times_recorded(&wrote, from, &id);
        assert!(!times.is_empty(), "the retry wrote the record");
        assert!(
            times.iter().all(|&n| n == 1),
            "recorded once each time: {times:?}"
        );
        chats.end_all();
    }

    #[test]
    fn a_retry_that_fails_again_keeps_the_chat_and_says_why() {
        let (chats, wrote) = recorded();
        chats.put_back_here(&one_recorded("/definitely/not/a/program"), SIZE);
        let id = waiting_id(&chats);

        let refused = chats
            .retry(&id, std::path::Path::new(HERE), SIZE)
            .expect_err("the program is still not there");

        assert_eq!(
            chats.would_not_start(),
            vec![NotStarted {
                id: id.clone(),
                name: "ide.7".to_owned(),
                why: refused,
                // A shell is on no profile, so it has nothing to approve.
                approval: None,
            }]
        );
        let last = lock(&wrote).len() - 1;
        assert_eq!(
            times_recorded(&wrote, last, &id),
            vec![1],
            "still recorded, once"
        );
    }

    /// A plane at `dir/plane` declaring the profile `work` (kind `claude`) in its local file,
    /// running a stand-in that waits, with `extra` after its program in its command. Nothing is
    /// approved. Answers the plane's root.
    fn a_plane_with_work(dir: &std::path::Path, extra: &str) -> std::path::PathBuf {
        let root = dir.join("plane");
        std::fs::create_dir_all(&root).expect("the plane");
        std::fs::write(root.join(purlis_core::plane::MANIFEST), "").expect("charter.toml");
        let program = a_claude(&root);
        std::fs::write(
            root.join(purlis_core::profiles::LOCAL_FILE),
            format!("[harness.work]\nkind = \"claude\"\ncommand = [{program:?}{extra}]\n"),
        )
        .expect("the profile");
        root
    }

    /// A record holding one chat, `ide.7`, on the profile `work`, standing in `root`.
    fn one_on_work(root: &std::path::Path) -> Record {
        Record {
            chats: vec![Chat {
                profile: Some("work".to_owned()),
                cwd: Some(root.to_path_buf()),
                ..chat("claude", "ide.7", None)
            }],
            ..one_recorded("claude")
        }
    }

    /// The profile `work` as the launch reads it at `root`.
    fn work_at(root: &std::path::Path) -> purlis_core::profiles::Profile {
        purlis_core::profiles::for_launch(root)
            .0
            .get("work")
            .expect("work is declared")
            .clone()
    }

    #[test]
    fn a_chat_waiting_on_a_profile_nobody_approved_says_which_line_needs_approving() {
        // #1246 (D-1246-5): the waiting chat's record says WHICH profile and WHICH exact line
        // need approval, so the window offers Review and approve… without reading the reason.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane_with_work(dir.path(), "");
        let (chats, _) = recorded();

        let open = chats.put_back(&one_on_work(&root), &root, SIZE);

        assert!(open.is_empty(), "an unapproved profile started");
        let waiting = chats.would_not_start();
        assert_eq!(waiting.len(), 1);
        assert_eq!(
            waiting[0].approval,
            Some(NeedsApproval {
                profile: "work".to_owned(),
                kind: "claude".to_owned(),
                source: purlis_core::profiles::Source::Local.as_str().to_owned(),
                approval: "new".to_owned(),
                // The very line the picker shows and `approve_profile` checks a click against.
                shown: purlis_core::profiletrust::shown(&root, &work_at(&root)),
            })
        );
        chats.end_all();
    }

    #[test]
    fn a_chat_waiting_on_a_profile_whose_command_changed_says_it_changed() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane_with_work(dir.path(), "");
        purlis_core::profiletrust::record_launched(
            &root,
            "work",
            &purlis_core::profiletrust::fingerprint(&work_at(&root)),
        )
        .expect("approved once");
        a_plane_with_work(dir.path(), ", \"--then-something-else\"");
        let (chats, _) = recorded();

        chats.put_back(&one_on_work(&root), &root, SIZE);

        let waiting = chats.would_not_start();
        let asked = waiting[0].approval.clone().expect("it asks again");
        assert_eq!(asked.approval, "changed");
        assert!(asked.shown.contains("--then-something-else"), "{asked:?}");
        chats.end_all();
    }

    #[test]
    fn a_waiting_chats_approval_shows_a_command_longer_than_the_display_limit_whole() {
        // #1014: Review and approve… asks with this line, so its last word reaches the
        // question however long the command is.
        let dir = tempfile::tempdir().expect("a directory");
        let filler = "x".repeat(purlis_core::shown::DISPLAY_LIMIT);
        let root = a_plane_with_work(dir.path(), &format!(", \"{filler}\", \"the-last-word\""));
        let (chats, _) = recorded();

        chats.put_back(&one_on_work(&root), &root, SIZE);

        let waiting = chats.would_not_start();
        let asked = waiting[0].approval.clone().expect("it asks");
        assert!(
            asked
                .shown
                .ends_with(&format!("{filler} the-last-word (kind claude)")),
            "{asked:?}"
        );
        assert!(!asked.shown.contains("..."), "clipped: {asked:?}");
        chats.end_all();
    }

    #[test]
    fn a_waiting_chat_whose_profile_is_approved_then_starts_on_retry_and_asks_nothing() {
        // The approval is recorded by `approve` against the line shown, and Retry now runs the
        // whole start again: nothing about the approval starts a chat on its own.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane_with_work(dir.path(), "");
        let (chats, _) = recorded();
        chats.put_back(&one_on_work(&root), &root, SIZE);
        let waiting = chats.would_not_start();
        let asked = waiting[0].approval.clone().expect("it asks");

        purlis_core::profiletrust::approve(&root, &work_at(&root), &asked.shown)
            .expect("the line shown is the line on disk");
        assert!(chats.open_now().is_empty(), "approving started the chat");
        chats
            .retry(&waiting[0].id, &root, SIZE)
            .expect("it starts now");

        assert!(chats.would_not_start().is_empty());
        chats.end_all();
    }

    #[test]
    fn a_retry_refused_for_approval_after_another_refusal_says_so_now() {
        // The approval is read again at every refused start, so a chat that first failed for
        // another reason offers it once a retry is refused for it.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane_with_work(dir.path(), "");
        let (chats, _) = recorded();
        let record = one_on_work(&root);
        chats.put_back(
            &Record {
                chats: vec![Chat {
                    profile: Some("not-declared-yet".to_owned()),
                    ..record.chats[0].clone()
                }],
                ..record
            },
            &root,
            SIZE,
        );
        let waiting = chats.would_not_start();
        assert_eq!(waiting[0].approval, None, "no such profile to approve");
        let local = root.join(purlis_core::profiles::LOCAL_FILE);

        std::fs::write(
            &local,
            std::fs::read_to_string(&local)
                .unwrap()
                .replace("[harness.work]", "[harness.not-declared-yet]"),
        )
        .unwrap();

        chats
            .retry(&waiting[0].id, &root, SIZE)
            .expect_err("not approved");

        let asked = chats.would_not_start()[0].approval.clone();
        assert_eq!(
            asked.map(|a| a.profile),
            Some("not-declared-yet".to_owned())
        );
        chats.end_all();
    }

    #[test]
    fn forget_drops_a_chat_that_did_not_start_from_the_record() {
        let (chats, wrote) = recorded();
        chats.put_back_here(&one_recorded("/definitely/not/a/program"), SIZE);

        chats
            .forget(&waiting_id(&chats))
            .expect("it is there to forget");

        assert!(chats.would_not_start().is_empty());
        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert!(last.chats.is_empty(), "the record no longer holds it");
    }

    #[test]
    fn of_two_waiting_chats_with_one_name_forget_drops_only_the_one_named_by_its_id() {
        // A split's chat takes its tab's name, and tab numbers start again at every launch:
        // two waiting chats can be called the same. Forget names one by its id (NO-3 review).
        let (chats, wrote) = recorded();
        chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![
                    chat("/definitely/not/a/program", "3", None),
                    chat("/definitely/not/a/program/either", "3", None),
                ],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );
        let waiting = chats.would_not_start();
        assert_eq!(waiting.len(), 2);
        assert_ne!(waiting[0].id, waiting[1].id, "each has its own id");

        chats.forget(&waiting[1].id).expect("the second is there");

        assert_eq!(chats.would_not_start(), vec![waiting[0].clone()]);
        let last = lock(&wrote).last().cloned().expect("a record was written");
        let ids: Vec<Option<String>> = last.chats.iter().map(|c| c.identity.id.clone()).collect();
        assert_eq!(
            ids,
            vec![Some(waiting[0].id.clone())],
            "the first is still recorded"
        );
    }

    #[test]
    fn a_record_holding_one_id_twice_puts_back_two_chats_with_their_own_ids_and_keeps_both() {
        // A record hand-edited, corrupt, or left by an older bug can hold two chats under one
        // id. The record writes one chat per id (the newest open), which is right only for a
        // retry or a fresh start under way — so the put-back gives the repeat an id of its own,
        // as it does a chat that had none, and neither drops out at the next write.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let shared = |name: &str| {
            let one = chat(&claude, name, None);
            Chat {
                identity: purlis_core::reopen::Identity {
                    id: Some(ID.to_owned()),
                    ..one.identity.clone()
                },
                ..one
            }
        };
        let (chats, wrote) = recorded();

        let open = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![shared("ide.7"), shared("ide.8")],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        assert_eq!(open.len(), 2, "both start");
        let last = lock(&wrote).last().cloned().expect("a record was written");
        let ids: Vec<Option<String>> = last.chats.iter().map(|c| c.identity.id.clone()).collect();
        assert_eq!(ids.len(), 2, "both are recorded: {ids:?}");
        assert_eq!(ids[0].as_deref(), Some(ID), "the first keeps the id");
        assert!(
            ids[1].is_some() && ids[1] != ids[0],
            "the repeat has its own: {ids:?}"
        );
        chats.end_all();
    }

    #[test]
    fn a_chat_that_is_not_waiting_to_start_cannot_be_retried_or_forgotten() {
        let (chats, _) = recorded();
        let here = std::path::Path::new(HERE);
        assert!(chats.forget(ID).is_err());
        assert!(chats.retry(ID, here, SIZE).is_err());
    }

    #[test]
    fn start_fresh_is_the_same_chat_in_a_new_run_recorded_once_while_both_run() {
        // NO-3: the plane-updated mark's Start fresh. The chat keeps its id and its name. The
        // old one stays open until the new one has started, and ending it is the caller's
        // (`Held::start_chat_fresh`) — in between, the record writes the newer, once.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        let open = chats.put_back_here(&one_recorded(&claude), SIZE);
        let was = open[0].session;
        let id = chats.record().chats[0].identity.id.clone().unwrap();

        let session = chats
            .start_fresh(was, std::path::Path::new(HERE), SIZE)
            .expect("it starts again");
        assert_ne!(session, was);
        let both = chats.record();
        assert_eq!(both.chats.len(), 1, "one chat, while two programs run");
        assert_eq!(both.chats[0].number, Some(session), "as the newer");
        chats.close(was).unwrap();

        let open: Vec<(u32, String)> = chats
            .open_now()
            .into_iter()
            .map(|one| (one.session, one.name))
            .collect();
        assert_eq!(open, vec![(session, "ide.7".to_owned())]);
        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(last.chats.len(), 1);
        assert_eq!(last.chats[0].identity.id.as_deref(), Some(id.as_str()));
        assert_ne!(
            last.chats[0].resume,
            Some(SessionId::new(ID).unwrap()),
            "a new conversation, not the one it was resuming"
        );
        chats.end_all();
    }

    #[test]
    fn a_refused_fresh_start_keeps_the_old_chat_open_and_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, _) = recorded();
        let open = chats.put_back_here(&one_recorded(&claude), SIZE);
        let was = open[0].session;
        // The program goes while the chat runs: a start of it now cannot happen.
        std::fs::remove_file(&claude).unwrap();

        chats
            .start_fresh(was, std::path::Path::new(HERE), SIZE)
            .expect_err("its program is gone");

        let open: Vec<u32> = chats
            .open_now()
            .into_iter()
            .map(|one| one.session)
            .collect();
        assert_eq!(open, vec![was], "still open");
        let names: Vec<String> = chats
            .record()
            .chats
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(names, vec!["ide.7"], "still recorded");
        chats.end_all();
    }

    #[test]
    fn a_chat_started_fresh_keeps_its_place_in_the_record_s_order() {
        // #1246: the window keeps the tab where it was, so the record must too. The new
        // session is not one the window has placed yet, and without this it went last.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (chats, wrote) = recorded();
        let a = chats.start(&chat(&claude, "a", None), SIZE).unwrap();
        let b = chats.start(&chat(&claude, "b", None), SIZE).unwrap();
        chats.hold_order(vec![a, b]);

        let again = chats
            .start_fresh(a, std::path::Path::new(HERE), SIZE)
            .expect("it starts again");
        chats.close(a).unwrap();

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(names_in(&last), ["a", "b"], "the next launch's strip");
        let open: Vec<u32> = chats
            .open_now()
            .into_iter()
            .map(|one| one.session)
            .collect();
        assert_eq!(open, vec![again, b], "a reloaded window's strip");
        chats.end_all();
    }

    #[test]
    fn a_chat_that_is_not_open_cannot_be_started_fresh() {
        let (chats, _) = recorded();
        assert!(
            chats
                .start_fresh(42, std::path::Path::new(HERE), SIZE)
                .is_err()
        );
    }

    #[test]
    fn a_chat_whose_program_has_gone_is_left_out_and_the_others_still_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let open = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![
                    chat("/definitely/not/a/program", "ide.7", Some(ID)),
                    chat(&claude, "ide.8", None),
                ],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        assert_eq!(open.len(), 1);
        assert_eq!(open[0].name, "ide.8");
    }

    // --- a chat keeps its number across a relaunch (charter-app#90) --------------------- //

    /// A plane with nothing selected anywhere, so the only rung that can answer about a
    /// workspace is the per-session pointer the tests below write.
    ///
    /// The same shape `sessions.rs` uses for charter-app#63, and for the same reason: these
    /// two defects are one defect at two levels, and a reproduction that let another rung
    /// answer would prove nothing about which chat a pointer belongs to.
    fn bare_plane() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a plane");
        let root = std::fs::canonicalize(dir.path()).expect("a resolved plane");
        std::fs::write(root.join("charter.toml"), "").expect("a manifest");
        std::fs::create_dir_all(root.join(".charter/sessions")).expect("a state directory");
        (dir, root)
    }

    /// Who a `charter` running inside chat `session` says it is.
    ///
    /// No pane id and no tty, which is the app's own case: a chat gets a pty of its own and
    /// none of `$TERM_SESSION_ID`/`$TMUX_PANE`/`$STY`/`$SSH_TTY`, so the per-session pointer
    /// is the only one there is and nothing catches a wrong key by accident.
    fn who_it_is(session: u32) -> purlis_core::active::Ids {
        let held = HashMap::from([(
            purlis_core::active::SESSION_ID_ENV.to_owned(),
            session.to_string(),
        )]);
        purlis_core::active::Ids::of(&|name| held.get(name).cloned())
    }

    /// `charter ws use <name>` from inside chat `session`, through the writer the command
    /// itself uses.
    fn picks(root: &std::path::Path, session: u32, name: &str) {
        use purlis_core::wscmd::select::{Scope, set_active};
        assert_eq!(
            set_active(root, name, &who_it_is(session), false),
            Scope::Session,
            "the selection did not land on the chat's own pointer"
        );
    }

    /// The workspace a `charter` inside chat `session` resolves, and the rung that answered.
    fn workspace_of(root: &std::path::Path, session: u32) -> purlis_core::active::ActiveWorkspace {
        purlis_core::active::workspace(&purlis_core::active::Asking {
            root,
            // Not inside any tree, so the cwd rung cannot answer and the pointers decide.
            cwd: root,
            flag: None,
            ids: &who_it_is(session),
            env: None,
        })
    }

    #[test]
    fn a_chat_that_comes_back_reads_the_workspace_it_picked_and_not_the_one_below_it() {
        // **The defect as the operator meets it** (charter-app#90). Two chats, each with a
        // workspace of its own. Close the first, quit, launch again: the second chat comes
        // back — and before this fix it came back as chat 1, reading the workspace the
        // CLOSED chat had picked and holding the lock that chat took. Nothing on screen says
        // so; the sidebar shows the chat the operator left, filed under a stranger's
        // workspace.
        //
        // It is the issue's headline shape turned around, and the turn matters: the report
        // was about a NEW chat inheriting a closed one's pointer, but a chat does not have to
        // be new. Numbers were dealt again at every launch in the order the record held, so
        // closing ANY chat shifted every later one down by one, and each of them landed on
        // the pointer of the chat that used to sit above it.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (_plane, root) = bare_plane();
        let chats = Chats::new();
        let first = chats.start(&chat(&claude, "ide.7", None), SIZE).unwrap();
        let second = chats.start(&chat(&claude, "ide.8", None), SIZE).unwrap();
        picks(&root, first, "finance");
        picks(&root, second, "ops");
        chats.close(first).expect("it closes");
        let record = chats.record();
        chats.end_all();

        let relaunched = Chats::new();
        let back = relaunched.put_back(&record, &root, SIZE);

        assert_eq!(
            back.len(),
            1,
            "the chat that was left open did not come back"
        );
        let found = workspace_of(&root, back[0].session);
        assert_eq!(
            found.name, "ops",
            "chat {} came back under number {} and read the workspace of the chat that closed",
            back[0].name, back[0].session
        );
        assert_eq!(
            found.rung,
            purlis_core::active::WorkspaceRung::SessionPointer,
            "it landed on 'ops' by some other rung, which proves nothing about the key"
        );
        relaunched.end_all();
    }

    #[test]
    fn a_new_chat_is_not_given_the_number_of_one_that_closed_before_the_quit() {
        // The half the issue reports in as many words: close a chat, and the number it was
        // using is free again at the next launch, while its `.charter/sessions/<n>.workspace`
        // and `<n>.lock` are still on disk — `wscmd::select`'s prune only drops them at 30
        // days. So the next chat the operator starts is filed in a workspace a chat they
        // closed had chosen, and is locked to it.
        //
        // Keeping each chat's number is not enough for this one. The closed chat is not in
        // the record at all, so nothing the chats say could hold the number; it is the
        // record's own counter that does (`Record::dealt`).
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let (_plane, root) = bare_plane();
        let chats = Chats::new();
        let staying = chats.start(&chat(&claude, "ide.7", None), SIZE).unwrap();
        let going = chats.start(&chat(&claude, "ide.8", None), SIZE).unwrap();
        picks(&root, staying, "finance");
        picks(&root, going, "ops");
        chats.close(going).expect("it closes");
        let record = chats.record();
        chats.end_all();

        let relaunched = Chats::new();
        relaunched.put_back(&record, &root, SIZE);
        let fresh = relaunched
            .start(&chat(&claude, "ide.9", None), SIZE)
            .unwrap();

        let found = workspace_of(&root, fresh);
        assert_eq!(
            found.name, "default",
            "a chat the operator just started was handed number {fresh}, which a closed chat \
             had already selected a workspace under"
        );
        assert_eq!(found.rung, purlis_core::active::WorkspaceRung::BuiltIn);
        relaunched.end_all();
    }

    #[test]
    fn the_record_keeps_the_number_each_chat_is_running_under() {
        // What the two tests above rest on, written out so a record that stops carrying it
        // fails here rather than only in a reproduction that takes a plane to see.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();
        let first = chats.start(&chat(&claude, "ide.7", None), SIZE).unwrap();
        let second = chats.start(&chat(&claude, "ide.8", None), SIZE).unwrap();

        let record = chats.record();

        let numbers: Vec<Option<u32>> = record.chats.iter().map(|one| one.number).collect();
        assert_eq!(numbers, vec![Some(first), Some(second)]);
        assert_eq!(record.dealt, second, "the counter is not what was dealt");
        chats.end_all();
    }

    #[test]
    fn the_counter_a_quit_records_counts_the_chats_that_closed_too() {
        // `dealt` is a high water mark and not a count of what is open. A record that wrote
        // the number of chats it holds would hand the next launch a number it had already
        // spent, which is the whole defect.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();
        chats.start(&chat(&claude, "ide.7", None), SIZE).unwrap();
        let going = chats.start(&chat(&claude, "ide.8", None), SIZE).unwrap();

        chats.close(going).expect("it closes");

        let record = chats.record();
        assert_eq!(record.chats.len(), 1);
        assert_eq!(record.dealt, going);
        chats.end_all();
    }

    #[test]
    fn a_record_written_before_chats_kept_their_numbers_is_put_back_as_it_always_was() {
        // Every operator has one of these at the first launch after this change, and it says
        // nothing about which chat was which. Dealing them in order is what the app did
        // before and the only honest answer; from that launch on they carry numbers.
        let dir = tempfile::tempdir().unwrap();
        let claude = a_claude(dir.path());
        let chats = Chats::new();

        let back = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![chat(&claude, "ide.7", None), chat(&claude, "ide.8", None)],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        let numbers: Vec<u32> = back.iter().map(|one| one.session).collect();
        assert_eq!(numbers, vec![1, 2]);
        chats.end_all();
    }

    #[test]
    fn a_chat_put_back_is_one_the_next_quit_records_again() {
        // A relaunch that lost the record would resume once and never again.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![chat(&a_claude(dir.path()), "ide.7", Some(ID))],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        let again = chats.record();

        assert_eq!(again.chats.len(), 1);
        assert_eq!(again.chats[0].resume, Some(SessionId::new(ID).unwrap()));
    }

    // ----- a pinned chat (ADR 0039, stored per ADR 0040) -----

    #[test]
    fn a_pin_is_written_into_the_record_so_it_outlives_the_app() {
        // The record is the only thing that says a chat exists at all, which is why the pin
        // is kept there and not in the machine store: a pin cannot outlive its chat.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.pin(session, true).expect("the chat is open");

        assert!(chats.record().chats[0].pinned);
        let _ = chats.close(session);
    }

    #[test]
    fn a_pin_can_be_taken_off_again() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();
        chats.pin(session, true).unwrap();

        chats.pin(session, false).unwrap();

        assert!(!chats.record().chats[0].pinned);
        let _ = chats.close(session);
    }

    #[test]
    fn a_chat_charter_does_not_have_open_cannot_be_pinned() {
        // A pin is an arrangement of what is there. Inventing an entry to hold one would put
        // a chat in the record that no start ever put there.
        let chats = Chats::new();

        assert!(chats.pin(7, true).is_err());
        assert_eq!(chats.record(), Record::default());
    }

    #[test]
    fn pinning_what_is_already_pinned_writes_nothing() {
        // The record is rewritten on every write and every write is a fingerprint the
        // machine store then has to vouch for — `bring_to_front` skips a no-op for the same
        // reason, and a pin pressed twice must not cost two writes.
        let dir = tempfile::tempdir().unwrap();
        let written = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = std::sync::Arc::clone(&written);
        let chats = Chats::recorded_by(Box::new(move |_| {
            counting.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();
        chats.pin(session, true).unwrap();
        let after_one = written.load(std::sync::atomic::Ordering::SeqCst);

        chats.pin(session, true).unwrap();

        assert_eq!(written.load(std::sync::atomic::Ordering::SeqCst), after_one);
        let _ = chats.close(session);
    }

    // ----- who a chat is beyond this clone, and its runs (ADR 0066, #834) -----

    const CHAT_ID: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7F";
    const DEVICE: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7G";
    const ELSEWHERE: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7J";
    const OLD_RUN: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7H";

    type Begun = std::sync::Arc<Mutex<Vec<(u32, String, String, Began)>>>;

    /// Chats on device [`DEVICE`] that keep every run they begin in the answer, oldest first.
    fn beginning(chats: &mut Chats) -> Begun {
        chats.on_device(Some(DEVICE.to_owned()));
        let begun: Begun = std::sync::Arc::default();
        let keep = std::sync::Arc::clone(&begun);
        chats.when_a_run_begins(Box::new(move |session, of, cause| {
            lock(&keep).push((session, of.chat.to_owned(), of.run.to_owned(), cause));
        }));
        begun
    }

    fn a_ulid(id: Option<&String>) -> bool {
        id.and_then(|id| purlis_core::reopen::a_ulid(id)).is_some()
    }

    /// What the core's start answers for a shell on no profile.
    fn a_shell_ready() -> purlis_core::start::Ready {
        purlis_core::start::Ready {
            program: "/bin/sh".to_owned(),
            command: Vec::new(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            harness: None,
            session: None,
            how: Reopened::Fresh(Fresh::NoConversationRecorded),
            plugins: std::collections::BTreeMap::new(),
            sandbox: None,
            unsandboxed: None,
            notices: Vec::new(),
            agents_md: Vec::new(),
        }
    }

    #[test]
    fn a_new_chat_is_given_an_id_on_this_device_and_a_first_run_the_record_holds() {
        let mut chats = Chats::new();
        let begun = beginning(&mut chats);

        let session = chats.start(&chat("/bin/sh", "shell", None), SIZE).unwrap();

        let identity = chats.record().chats[0].identity.clone();
        assert!(a_ulid(identity.id.as_ref()), "{identity:?}");
        assert_eq!(identity.device.as_deref(), Some(DEVICE));
        assert!(a_ulid(identity.run.as_ref()), "{identity:?}");
        assert_eq!(
            *lock(&begun),
            vec![(
                session,
                identity.id.clone().unwrap(),
                identity.run.clone().unwrap(),
                Began::Start
            )]
        );
        let _ = chats.close(session);
    }

    #[test]
    fn a_chat_put_back_after_a_relaunch_keeps_its_id_and_begins_a_reopen_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut chats = Chats::new();
        let begun = beginning(&mut chats);
        let was = purlis_core::reopen::Identity {
            id: Some(CHAT_ID.to_owned()),
            device: Some(ELSEWHERE.to_owned()),
            run: Some(OLD_RUN.to_owned()),
            resumed_from: None,
        };

        let open = chats.put_back_here(
            &Record {
                chats: vec![Chat {
                    identity: was.clone(),
                    ..chat(&a_claude(dir.path()), "ide.7", Some(ID))
                }],
                ..Record::default()
            },
            SIZE,
        );

        let now = chats.record().chats[0].identity.clone();
        assert_eq!(now.id.as_deref(), Some(CHAT_ID), "the same chat");
        assert_eq!(
            now.device.as_deref(),
            Some(ELSEWHERE),
            "the origin device is the one that minted it"
        );
        assert_ne!(now.run.as_deref(), Some(OLD_RUN), "a run of its own");
        assert_eq!(
            *lock(&begun),
            vec![(
                open[0].session,
                CHAT_ID.to_owned(),
                now.run.clone().unwrap(),
                Began::Reopen
            )]
        );
    }

    #[test]
    fn a_chat_recorded_before_ids_is_given_one_at_the_launch_that_reads_it_in_a_reopen_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut chats = Chats::new();
        let begun = beginning(&mut chats);

        chats.put_back_here(
            &Record {
                chats: vec![chat(&a_claude(dir.path()), "ide.7", Some(ID))],
                ..Record::default()
            },
            SIZE,
        );

        let now = chats.record().chats[0].identity.clone();
        assert!(a_ulid(now.id.as_ref()));
        assert_eq!(now.device.as_deref(), Some(DEVICE));
        assert_eq!(lock(&begun)[0].3, Began::Reopen, "ADR 0066's migration");
    }

    #[test]
    fn a_chat_a_rename_left_without_its_conversation_comes_back_in_a_fresh_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut chats = Chats::new();
        let begun = beginning(&mut chats);

        chats.put_back_here(
            &Record {
                chats: vec![Chat {
                    renamed_from: Some("alpha".to_owned()),
                    identity: purlis_core::reopen::Identity {
                        id: Some(CHAT_ID.to_owned()),
                        ..Default::default()
                    },
                    ..chat(&a_claude(dir.path()), "ide.7", None)
                }],
                ..Record::default()
            },
            SIZE,
        );

        assert_eq!(lock(&begun)[0].1, CHAT_ID);
        assert_eq!(lock(&begun)[0].3, Began::Fresh);
    }

    #[test]
    fn a_chat_started_again_in_place_of_one_that_lost_its_conversation_is_that_chat_in_a_fresh_run()
    {
        let mut chats = Chats::new();
        let begun = beginning(&mut chats);
        let first = chats.start(&chat("/bin/sh", "ide.7", None), SIZE).unwrap();
        let was = chats.record().chats[0].identity.clone();
        // The window closes the tab before it asks for the fresh start, so the chat it names
        // may already be gone.
        chats.close(first).unwrap();

        let again = chats
            .start_ready_instead_of(
                first,
                &chat("/bin/sh", "ide.7", None),
                &a_shell_ready(),
                SIZE,
            )
            .unwrap();

        let now = chats.record().chats[0].identity.clone();
        assert_eq!(now.id, was.id, "the same chat");
        assert_ne!(now.run, was.run);
        let last = lock(&begun).last().cloned().unwrap();
        assert_eq!((last.0, last.3), (again, Began::Fresh));
        let _ = chats.close(again);
    }

    #[test]
    fn a_chat_started_again_before_the_window_closed_the_old_one_is_recorded_once() {
        // #856 review F3: the window's close and its fresh start are not ordered, so the start
        // takes the old chat out of what is recorded itself.
        let (chats, wrote) = recorded();
        let first = chats.start(&chat("/bin/sh", "ide.7", None), SIZE).unwrap();
        let was = chats.record().chats[0].identity.id.clone();

        let again = chats
            .start_ready_instead_of(
                first,
                &chat("/bin/sh", "ide.7", None),
                &a_shell_ready(),
                SIZE,
            )
            .unwrap();

        for record in lock(&wrote).iter() {
            let with_it = record
                .chats
                .iter()
                .filter(|one| one.identity.id == was)
                .count();
            assert!(with_it <= 1, "two chats under one id: {record:#?}");
        }
        assert_eq!(chats.record().chats.len(), 1);
        let _ = chats.close(first);
        let _ = chats.close(again);
    }

    #[test]
    fn a_chat_recorded_before_ids_begins_a_reopen_run_even_with_no_conversation_to_resume() {
        // ADR 0066's migration: "Its first run after the upgrade has `cause: reopen`."
        let dir = tempfile::tempdir().unwrap();
        let mut chats = Chats::new();
        let begun = beginning(&mut chats);

        let open = chats.put_back_here(
            &Record {
                chats: vec![chat(&a_claude(dir.path()), "ide.7", None)],
                ..Record::default()
            },
            SIZE,
        );

        assert_eq!(open[0].how, Reopened::Fresh(Fresh::NoConversationRecorded));
        assert_eq!(lock(&begun)[0].3, Began::Reopen);
    }

    #[test]
    fn a_run_the_host_moved_the_chat_onto_is_the_one_the_record_holds() {
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.follow_conversation(session, ID, Some(OLD_RUN));

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(last.chats[0].identity.run.as_deref(), Some(OLD_RUN));
        let _ = chats.close(session);
    }

    // ----- the conversation a chat is in now (Q10) -----

    #[test]
    fn a_conversation_the_chat_s_harness_moved_to_is_the_one_the_record_resumes() {
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.follow_conversation(session, ID, None);

        let last = lock(&wrote).last().cloned().expect("a record was written");
        assert_eq!(last.chats[0].resume, Some(SessionId::new(ID).unwrap()));
        let _ = chats.close(session);
    }

    #[test]
    fn following_the_conversation_the_record_already_has_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (chats, wrote) = recorded();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "ide.7", Some(ID)), SIZE)
            .unwrap();
        let before = lock(&wrote).len();

        chats.follow_conversation(session, ID, None);

        assert_eq!(lock(&wrote).len(), before);
        let _ = chats.close(session);
    }

    #[test]
    fn a_shell_tab_is_resumed_by_no_conversation_whatever_ran_in_it() {
        // A shell tab resumes nothing (`Fresh::NoResumeForThisProgram`), so an id in its record
        // would be a write that means nothing.
        let (chats, wrote) = recorded();
        let session = chats.start(&chat("/bin/sh", "shell", None), SIZE).unwrap();
        let before = lock(&wrote).len();

        chats.follow_conversation(session, ID, None);

        assert_eq!(lock(&wrote).len(), before);
        assert_eq!(chats.record().chats[0].resume, None);
        let _ = chats.close(session);
    }

    #[test]
    fn a_conversation_for_a_chat_that_is_not_open_invents_nothing() {
        let (chats, wrote) = recorded();

        chats.follow_conversation(7, ID, None);

        assert!(lock(&wrote).is_empty());
        assert_eq!(chats.record(), Record::default());
    }

    // ----- the name the operator gave a chat (charter-app#254) -----

    #[test]
    fn a_name_given_to_a_chat_is_written_into_the_record_and_said_on_it() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "3", None), SIZE)
            .unwrap();

        let held = chats
            .rename(session, "  billing bug ")
            .expect("the chat is open");

        assert_eq!(held.as_deref(), Some("billing bug"));
        assert_eq!(
            chats.record().chats[0].label.as_deref(),
            Some("billing bug")
        );
        assert_eq!(chats.open_now()[0].label.as_deref(), Some("billing bug"));
        let _ = chats.close(session);
    }

    #[test]
    fn a_rename_is_charters_label_and_leaves_the_harness_name_alone() {
        // The harness was started with `--name 3` and is resumed under it: a running harness
        // is never disturbed by a rename, and a split still starts its chat as `3`.
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "3", None), SIZE)
            .unwrap();

        chats.rename(session, "billing bug").unwrap();

        assert_eq!(chats.record().chats[0].name, "3");
        assert_eq!(chats.open_now()[0].name, "3");
        let _ = chats.close(session);
    }

    #[test]
    fn a_blank_name_takes_the_given_one_off() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "3", None), SIZE)
            .unwrap();
        chats.rename(session, "billing bug").unwrap();

        let held = chats.rename(session, "   ").unwrap();

        assert_eq!(held, None);
        assert_eq!(chats.record().chats[0].label, None);
        let _ = chats.close(session);
    }

    #[test]
    fn a_name_charter_refuses_changes_nothing_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let session = chats
            .start(&chat(&a_claude(dir.path()), "3", None), SIZE)
            .unwrap();
        chats.rename(session, "billing bug").unwrap();

        let refused = chats.rename(session, "pay\u{202e}lanigiro").unwrap_err();

        assert!(refused.contains("invisible"), "{refused}");
        assert_eq!(
            chats.record().chats[0].label.as_deref(),
            Some("billing bug")
        );
        let _ = chats.close(session);
    }

    #[test]
    fn a_chat_charter_does_not_have_open_cannot_be_renamed() {
        let chats = Chats::new();

        assert!(chats.rename(7, "billing bug").is_err());
        assert_eq!(chats.record(), Record::default());
    }

    #[test]
    fn renaming_to_the_name_it_already_has_writes_nothing() {
        // `pin`'s reason: every write is a fingerprint the machine store has to vouch for.
        let dir = tempfile::tempdir().unwrap();
        let written = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = std::sync::Arc::clone(&written);
        let chats = Chats::recorded_by(Box::new(move |_| {
            counting.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        let session = chats
            .start(&chat(&a_claude(dir.path()), "3", None), SIZE)
            .unwrap();
        chats.rename(session, "billing bug").unwrap();
        let after_one = written.load(std::sync::atomic::Ordering::SeqCst);

        chats.rename(session, " billing bug").unwrap();

        assert_eq!(written.load(std::sync::atomic::Ordering::SeqCst), after_one);
        let _ = chats.close(session);
    }

    #[test]
    fn a_chat_put_back_keeps_the_name_it_was_given() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        let back = chats.put_back_here(
            &Record {
                views: Vec::new(),
                chats: vec![Chat {
                    label: Some("billing bug".into()),
                    ..chat(&a_claude(dir.path()), "3", None)
                }],
                dealt: 0,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
            SIZE,
        );

        assert_eq!(back[0].label.as_deref(), Some("billing bug"));
        assert_eq!(
            chats.record().chats[0].label.as_deref(),
            Some("billing bug")
        );
        chats.end_all();
    }

    #[test]
    fn ending_every_chat_leaves_nothing_to_record() {
        let dir = tempfile::tempdir().unwrap();
        let chats = Chats::new();
        chats
            .start(&chat(&a_claude(dir.path()), "ide.7", None), SIZE)
            .unwrap();

        chats.end_all();

        let record = chats.record();
        assert_eq!(record.chats, vec![]);
        assert_eq!(chats.sessions().running(), Vec::<u32>::new());
        // The counter is NOT reset with them. A chat that ends does not give its number
        // back: `.charter/sessions/1.workspace` outlives it by 30 days, and a later chat
        // dealt 1 again would read it (charter-app#90).
        assert_eq!(record.dealt, 1);
    }
    #[test]
    fn the_record_keeps_the_profile_persona_and_footer_a_chat_was_started_on() {
        // `record()` rebuilds each chat with `..chat.clone()`, so these ride along — which
        // means nothing says so when they stop. A struct literal that names one field and
        // spreads the rest is exactly where a later edit drops one silently, and an edit
        // that added `profile: None` beside the spread would do it: the record would still
        // be written, still be read, and every chat would come back as a shell.
        //
        // The footer choice (ADR 0029) rides the same spread and fails the same way:
        // it would be dropped at the quit and the chat would come back blanked.
        let chats = Chats::new();
        let chat = Chat {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 30".to_owned()],
            cwd: None,
            name: "ide.7".to_owned(),
            resume: None,
            active: false,
            profile: Some("claude-work".to_owned()),
            persona: Some("steward".to_owned()),
            show_footer: true,
            pinned: false,
            number: None,
            label: None,
            from: None,
            renamed_from: None,
            ..Default::default()
        };

        let session = chats
            .start(
                &chat,
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .unwrap();
        let record = chats.record();

        assert_eq!(record.chats.len(), 1);
        assert_eq!(record.chats[0].profile.as_deref(), Some("claude-work"));
        assert_eq!(record.chats[0].persona.as_deref(), Some("steward"));
        assert!(record.chats[0].show_footer);
        let _ = chats.close(session);
    }
    #[test]
    fn the_sidebar_is_told_the_harness_a_chat_was_started_as_not_one_read_off_its_program() {
        // A profile's command is commonly a WRAPPER (ADR 0022), and `Harness::of_command`
        // answers `None` for one — the same answer it gives a shell, deliberately. The board
        // is told the profile's declared kind at the start; the sidebar used to ask the
        // program's name instead and say "no harness" for the very same chat. A scenario
        // test caught the disagreement; this is what keeps them one answer.
        let chats = Chats::new();
        let chat = Chat {
            program: "/bin/sh".to_owned(),
            args: Vec::new(),
            cwd: None,
            name: "ide.7".to_owned(),
            resume: None,
            active: false,
            profile: Some("claude-work".to_owned()),
            persona: None,
            show_footer: false,
            pinned: false,
            number: None,
            label: None,
            from: None,
            renamed_from: None,
            ..Default::default()
        };
        assert_eq!(
            chat.harness(),
            None,
            "the premise: the program is not a harness"
        );
        let ready = purlis_core::start::Ready {
            program: "/bin/sh".to_owned(),
            command: vec!["-c".to_owned(), "sleep 30".to_owned()],
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            harness: Some(Harness::ClaudeCode),
            session: None,
            how: purlis_core::reopen::Reopened::Fresh(
                purlis_core::reopen::Fresh::NoConversationRecorded,
            ),
            plugins: std::collections::BTreeMap::new(),
            sandbox: None,
            unsandboxed: None,
            notices: Vec::new(),
            agents_md: Vec::new(),
        };

        let session = chats
            .start_ready(
                &chat,
                &ready,
                Size {
                    columns: 80,
                    rows: 24,
                },
            )
            .unwrap();

        let open = chats.open_now();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].harness, Some(Harness::ClaudeCode));
        assert_eq!(open[0].profile.as_deref(), Some("claude-work"));
        let _ = chats.close(session);
    }
    #[test]
    fn the_board_is_told_the_harness_the_profile_declared_before_the_program_starts() {
        // The board judges every report against the harness it was told at the start, and a
        // chat on a wrapper profile would otherwise be told `None` — the narrowest rule
        // there is — while the sidebar said Claude Code. One announcement, one answer.
        //
        // Before the program starts, because a harness fires `SessionStart` at its own exec
        // and a board that learned the chat's number afterwards would miss it.
        let told = std::sync::Arc::new(Mutex::new(
            Vec::<(u32, Option<Harness>, Option<String>)>::new(),
        ));
        let chats = Chats::new();
        {
            let told = std::sync::Arc::clone(&told);
            chats.when_one_starts(Box::new(move |session, harness, conversation| {
                lock(&told).push((session, harness, conversation));
            }));
        }
        let chat = Chat {
            program: "/bin/sh".to_owned(),
            args: Vec::new(),
            cwd: None,
            name: "ide.7".to_owned(),
            resume: None,
            active: false,
            profile: Some("claude-work".to_owned()),
            persona: Some("steward".to_owned()),
            show_footer: false,
            pinned: false,
            number: None,
            label: None,
            from: None,
            renamed_from: None,
            ..Default::default()
        };
        assert_eq!(
            chat.harness(),
            None,
            "the premise: the program is not a harness"
        );
        let ready = purlis_core::start::Ready {
            program: "/bin/sh".to_owned(),
            command: vec!["-c".to_owned(), "sleep 30".to_owned()],
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            harness: Some(Harness::ClaudeCode),
            session: purlis_core::harness::SessionId::new(ID).ok(),
            how: purlis_core::reopen::Reopened::Fresh(
                purlis_core::reopen::Fresh::NoConversationRecorded,
            ),
            plugins: std::collections::BTreeMap::new(),
            sandbox: None,
            unsandboxed: None,
            notices: Vec::new(),
            agents_md: Vec::new(),
        };

        let session = chats.start_ready(&chat, &ready, SIZE).unwrap();

        let told = lock(&told).clone();
        assert_eq!(
            told,
            vec![(session, Some(Harness::ClaudeCode), Some(ID.to_owned()))],
            "the board was told something other than the profile's declared kind"
        );
        let _ = chats.close(session);
    }

    /// The words a profile chat's program was started with, one to an element, once the
    /// profile's command is `command` (its first word replaced by a stand-in that writes its
    /// arguments down) and the app arms it with a plugin.
    fn argv_of_a_profile_chat(command: &[&str]) -> (Vec<String>, String) {
        argv_of_a_chat_in(command, "", |_| String::new())
    }

    /// The same, in a plane whose `charter.toml` is `shared`, with `profile(root)` written
    /// under the profile's table in `charter.local.toml`.
    fn argv_of_a_chat_in(
        command: &[&str],
        shared: &str,
        profile: impl Fn(&std::path::Path) -> String,
    ) -> (Vec<String>, String) {
        let dir = tempfile::tempdir().expect("a directory");
        let root = dir.path().join("plane");
        std::fs::create_dir_all(&root).expect("the plane");
        std::fs::write(root.join(purlis_core::plane::MANIFEST), shared).expect("charter.toml");
        let argv = root.join("argv");
        let running = root.join("running");
        let program = stand_in::program(
            &root,
            command[0],
            &format!(
                "#!/bin/sh\n: > {running:?}\n\
                 for a in \"$@\"; do printf '%s\\n' \"$a\"; done > {argv:?}.part\n\
                 mv {argv:?}.part {argv:?}\n"
            ),
        );
        let mut words = vec![format!("{:?}", program.display().to_string())];
        words.extend(command[1..].iter().map(|w| format!("{w:?}")));
        std::fs::write(
            root.join(purlis_core::profiles::LOCAL_FILE),
            format!(
                "[harness.work]\nkind = \"claude\"\ncommand = [{}]\n{}",
                words.join(", "),
                profile(&root)
            ),
        )
        .expect("the profile");
        let set = purlis_core::profiles::current(&root);
        purlis_core::profiletrust::record_launched(
            &root,
            "work",
            &purlis_core::profiletrust::fingerprint(set.get("work").expect("it reads")),
        )
        .expect("approved");

        let plugin = root.join("plugin");
        let mut chats = Chats::new();
        chats.arming_with(crate::Shipped {
            binary: Some(root.join("charter")),
            plugin: Some(plugin.clone()),
            shims: None,
            git_hooks: None,
        });
        let ready = purlis_core::start::ready(
            &purlis_core::start::Start {
                profile: Some("work".to_owned()),
                persona: None,
                name: "ide.7".to_owned(),
                cwd: Some(root.clone()),
                resume: None,
                show_footer: false,
                resuming: None,
                without_sandbox: None,
                grants: Default::default(),
            },
            &root,
        )
        .expect("the chat starts");
        let chat = Chat {
            program: ready.program.clone(),
            args: Vec::new(),
            cwd: ready.cwd.clone(),
            name: "ide.7".to_owned(),
            resume: ready.session.clone(),
            active: false,
            profile: Some("work".to_owned()),
            persona: None,
            show_footer: false,
            pinned: false,
            number: None,
            label: None,
            from: None,
            renamed_from: None,
            ..Default::default()
        };
        let session = chats.start_ready(&chat, &ready, SIZE).expect("it runs");
        // The deadline that matters counts from the stand-in running (#1138).
        let spawned = std::time::Instant::now();
        while !running.exists() && spawned.elapsed() < TO_START {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let ran = std::time::Instant::now();
        while running.exists() && !argv.exists() && ran.elapsed() < ONCE_RUNNING {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = chats.close(session);
        assert!(
            running.exists(),
            "the stand-in did not start in {TO_START:?}"
        );
        let said = std::fs::read_to_string(&argv).expect("the stand-in ran");
        (
            said.lines().map(str::to_owned).collect(),
            plugin.display().to_string(),
        )
    }

    #[test]
    fn a_wrapper_profile_keeps_its_own_words_first_and_the_apps_come_after_them() {
        // M8.3: a profile's command is commonly a WRAPPER whose first argument is its own
        // subcommand (`ccs work`). The app's flags used to go straight after argv[0], which
        // started `ccs --plugin-dir … --settings … work` and broke the wrapper.
        let (argv, plugin) = argv_of_a_profile_chat(&["ccs", "work"]);
        assert_eq!(argv.first().map(String::as_str), Some("work"), "{argv:?}");
        assert_eq!(argv[1..3], ["--plugin-dir".to_owned(), plugin], "{argv:?}");
        assert_eq!(argv[3], "--mcp-config", "{argv:?}");
        assert_eq!(argv[5], "--settings", "{argv:?}");
        // The app's own session words come after the flags, where they always were.
        assert_eq!(argv[7], "--session-id", "{argv:?}");
        assert_eq!(argv[9..], ["--name", "ide.7"], "{argv:?}");
    }

    #[test]
    fn a_claude_code_chat_runs_with_the_plugins_its_project_chose() {
        // charter-app#274: the words the program actually received. The project turns one of
        // the account's installed plugins off; the other is left to Claude Code, and the two
        // pins ride beside it as they always have.
        let (argv, _) = argv_of_a_chat_in(
            &["claude"],
            "[harness_plugins.claude]\n\"figma@official\" = false\n",
            |root| {
                let config = root.join("claude-config");
                std::fs::create_dir_all(config.join("plugins")).expect("the config dir");
                std::fs::write(
                    config.join("plugins/installed_plugins.json"),
                    r#"{"version": 2, "plugins": {"figma@official": [{"scope": "user"}],
                        "serena@official": [{"scope": "user"}]}}"#,
                )
                .expect("the install record");
                format!(
                    "env = {{ CLAUDE_CONFIG_DIR = {:?} }}\n",
                    config.display().to_string()
                )
            },
        );
        let at = argv
            .iter()
            .position(|word| word == "--settings")
            .expect("--settings");
        let settings: serde_json::Value = serde_json::from_str(&argv[at + 1]).expect("JSON");
        assert_eq!(
            settings["enabledPlugins"],
            serde_json::json!({
                "purlis@inline": true,
                "charter@charter": false,
                "charter@inline": false,
                "charter-app@inline": false,
                "charter@charter-app": false,
                "figma@official": false,
            }),
            "{argv:?}"
        );
    }

    #[test]
    fn a_plain_profile_is_started_exactly_as_before() {
        let (argv, plugin) = argv_of_a_profile_chat(&["claude"]);
        assert_eq!(argv[..2], ["--plugin-dir".to_owned(), plugin], "{argv:?}");
        assert_eq!(argv[2], "--mcp-config", "{argv:?}");
        assert_eq!(argv[4], "--settings", "{argv:?}");
        assert_eq!(argv[6], "--session-id", "{argv:?}");
        assert_eq!(argv[8..], ["--name", "ide.7"], "{argv:?}");
    }

    // --- charter's git hooks in a chat (SQ-16) ----------------------------------------------- //

    fn armed_with_git_hooks() -> Chats {
        let mut chats = Chats::new();
        chats.arming_with(crate::Shipped {
            git_hooks: Some(purlis_core::githooks::GitHooks::at("/app/data/git-hooks")),
            ..crate::Shipped::default()
        });
        chats
    }

    #[test]
    fn a_harness_chat_commits_through_charters_git_hooks() {
        assert_eq!(
            armed_with_git_hooks().git_hooks_for(Some(Harness::ClaudeCode)),
            Some(purlis_core::githooks::GitHooks::at("/app/data/git-hooks"))
        );
    }

    #[test]
    fn a_shell_tab_and_an_app_without_git_hooks_arm_nothing() {
        assert_eq!(armed_with_git_hooks().git_hooks_for(None), None);
        assert_eq!(Chats::new().git_hooks_for(Some(Harness::Codex)), None);
    }

    // --- a shell tab's shims (SI-5, ADR 0062) ---------------------------------------------- //

    fn armed_with_shims() -> Chats {
        let mut chats = Chats::new();
        chats.arming_with(crate::Shipped {
            binary: None,
            plugin: None,
            shims: Some(purlis_core::shellguard::Shims::at("/app/data/shims")),
            git_hooks: None,
        });
        chats
    }

    fn path_of(env: &[(String, String)]) -> Option<&str> {
        env.iter()
            .find(|(name, _)| name == "PATH")
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn a_shell_tab_finds_charters_shims_first_on_its_path() {
        let chats = armed_with_shims();

        let (args, env) = chats.shell_start(&chat("/bin/sh", "shell", None), "/bin/sh", vec![]);

        assert!(args.is_empty(), "{args:?}");
        let path = path_of(&env).expect("a PATH");
        assert!(path.starts_with("/app/data/shims/bin:"), "{path}");
    }

    #[test]
    fn a_zsh_shell_tab_is_pointed_at_charters_start_files_and_keeps_its_own_arguments() {
        let chats = armed_with_shims();
        let mut shell = chat("/bin/zsh", "shell", None);
        shell.args = vec!["-l".to_owned()];

        let (args, env) = chats.shell_start(&shell, "/bin/zsh", shell.args.clone());

        assert_eq!(args, ["-l"]);
        assert!(
            env.contains(&("ZDOTDIR".to_owned(), "/app/data/shims/zsh".to_owned())),
            "{env:?}"
        );
    }

    #[test]
    fn a_harness_chat_never_gets_the_shims() {
        let chats = armed_with_shims();

        let (args, env) = chats.shell_start(&chat("claude", "1", None), "claude", vec![]);

        assert!(args.is_empty());
        assert!(env.is_empty(), "{env:?}");
    }

    #[test]
    fn a_chat_on_a_profile_never_gets_the_shims() {
        let chats = armed_with_shims();
        let mut on_a_profile = chat("/usr/local/bin/wrapper", "1", None);
        on_a_profile.profile = Some("work".to_owned());

        let (_, env) = chats.shell_start(&on_a_profile, "/usr/local/bin/wrapper", vec![]);

        assert!(env.is_empty(), "{env:?}");
    }

    #[test]
    fn a_shell_tab_in_an_app_with_no_shims_is_a_plain_shell() {
        let chats = Chats::new();

        let (args, env) = chats.shell_start(&chat("/bin/zsh", "shell", None), "/bin/zsh", vec![]);

        assert!(args.is_empty());
        assert!(env.is_empty(), "{env:?}");
    }

    // --- the session host is a seam (FD-3) ------------------------------------------- //

    #[test]
    fn a_chat_runs_on_whichever_session_host_the_chats_were_given() {
        let host = Pretend::default();
        host.already_dealt(6);
        let chats = Chats::on_host(Box::new(|_| {}), Box::new(host.clone()));

        let session = chats
            .start(
                &chat("/nowhere/a-program-nothing-runs", "hosted", None),
                SIZE,
            )
            .expect("the host opens it");

        assert_eq!(session, 7, "the number is the host's to deal");
        assert_eq!(
            host.asked(),
            vec![(7, "/nowhere/a-program-nothing-runs".to_owned())]
        );
        assert_eq!(
            chats
                .open_now()
                .iter()
                .map(|one| one.session)
                .collect::<Vec<_>>(),
            vec![7]
        );
        assert_eq!(chats.record().dealt, 7);

        chats.close(session).expect("the host ends it");

        assert_eq!(host.running(), Vec::<u32>::new());
        assert!(chats.open_now().is_empty());
    }

    // ----- the sandbox's audit and count (ADR 0067 §7, ruling V78) -----

    /// What each start told `when_the_sandbox_is_decided`, and when each run began, in one
    /// list in the order they were said.
    type Said = std::sync::Arc<Mutex<Vec<String>>>;

    fn saying(chats: &mut Chats) -> Said {
        saying_or(chats, None)
    }

    /// [`saying`], with every trust event refused for `refused` when one is given: what the
    /// app's callback answers when the event log cannot take it.
    fn saying_or(chats: &mut Chats, refused: Option<&'static str>) -> Said {
        let said: Said = std::sync::Arc::default();
        let runs = std::sync::Arc::clone(&said);
        chats.when_a_run_begins(Box::new(move |session, _, cause| {
            lock(&runs).push(format!("{session} run {}", cause.word()));
        }));
        let decided = std::sync::Arc::clone(&said);
        chats.when_the_sandbox_is_decided(Box::new(move |it| {
            let what = match (it.change, it.counted) {
                (Some(change), _) => {
                    if let Some(why) = refused {
                        return Err(why.to_owned());
                    }
                    assert!(it.run.is_some(), "a trust event names the run it is under");
                    change.kind().to_owned()
                }
                (None, Some(counted)) => format!("counted {counted:?}"),
                (None, None) => "nothing".to_owned(),
            };
            lock(&decided).push(format!(
                "{what} {} {}",
                it.harness.map_or("-", Harness::name),
                it.persona.unwrap_or("-")
            ));
            Ok(())
        }));
        said
    }

    fn a_person_lifted_it() -> purlis_core::sandbox::Lifted {
        purlis_core::sandbox::Lifted {
            by: purlis_core::sandbox::By::Person,
            reason: Some("needs the network".to_owned()),
        }
    }

    #[test]
    fn a_chat_started_without_the_sandbox_is_audited_off_as_its_run_begins_and_counted_once_up() {
        let mut chats = Chats::new();
        let said = saying(&mut chats);
        let ready = purlis_core::start::Ready {
            harness: Some(Harness::ClaudeCode),
            unsandboxed: Some(a_person_lifted_it()),
            ..a_shell_ready()
        };
        let chat = Chat {
            persona: Some("steward".to_owned()),
            ..chat("/bin/sh", "c", None)
        };

        let session = chats.start_ready(&chat, &ready, SIZE).expect("starts");

        assert_eq!(
            *lock(&said),
            [
                "trust.sandbox.off claude steward".to_owned(),
                format!("{session} run start"),
                "counted OptedOut claude steward".to_owned(),
            ],
            "the record is written before the chat's program runs"
        );
        assert!(
            chats.record().chats[0].unsandboxed,
            "the record says what the run ran under"
        );
        let _ = chats.close(session);
    }

    #[test]
    fn a_chat_whose_last_run_was_unsandboxed_is_audited_back_on_when_it_starts_sandboxed() {
        let plane = a_sandboxed_plane();
        let mut chats = Chats::on_host(Box::new(|_| {}), Box::new(Pretend::default()));
        chats.arming_with(crate::Shipped {
            binary: Some(plane.path().join("charter")),
            plugin: Some(plane.path().join("plugin")),
            shims: None,
            git_hooks: None,
        });
        let said = saying(&mut chats);
        let ready = ready_under(Harness::ClaudeCode, a_claude_sandbox(plane.path()));
        let chat = Chat {
            unsandboxed: true,
            cwd: Some(plane.path().to_path_buf()),
            ..chat("/bin/sh", "c", None)
        };

        let session = chats
            .start_ready_as(&chat, &ready, SIZE, Why::Relaunch)
            .expect("starts");

        assert_eq!(
            *lock(&said),
            [
                "trust.sandbox.on claude -".to_owned(),
                format!("{session} run fresh"),
            ],
            "a relaunch is never counted as a new chat"
        );
        assert!(!chats.record().chats[0].unsandboxed);
    }

    #[test]
    fn a_new_sandboxed_chat_is_counted_and_has_nothing_to_audit() {
        let plane = a_sandboxed_plane();
        let mut chats = Chats::on_host(Box::new(|_| {}), Box::new(Pretend::default()));
        chats.arming_with(crate::Shipped {
            binary: Some(plane.path().join("charter")),
            plugin: Some(plane.path().join("plugin")),
            shims: None,
            git_hooks: None,
        });
        let said = saying(&mut chats);
        let ready = ready_under(Harness::ClaudeCode, a_claude_sandbox(plane.path()));

        let chat = Chat {
            cwd: Some(plane.path().to_path_buf()),
            ..chat("/bin/sh", "c", None)
        };

        let session = chats.start_ready(&chat, &ready, SIZE).expect("starts");

        assert_eq!(
            *lock(&said),
            [
                format!("{session} run start"),
                "counted Sandboxed claude -".to_owned(),
            ]
        );
    }

    #[test]
    fn a_shell_says_nothing_about_a_sandbox() {
        let mut chats = Chats::new();
        let said = saying(&mut chats);

        let session = chats.start(&chat("/bin/sh", "shell", None), SIZE).unwrap();

        assert_eq!(*lock(&said), [format!("{session} run start")]);
        let _ = chats.close(session);
    }

    /// ADR 0067 §7: an opt-out is never unaudited. A person's is refused, and nothing runs,
    /// when its trust event cannot be written.
    #[test]
    fn a_persons_opt_out_that_cannot_be_recorded_is_not_started() {
        let host = Pretend::default();
        let mut chats = Chats::on_host(Box::new(|_| {}), Box::new(host.clone()));
        let said = saying_or(&mut chats, Some("the disk is full"));
        let ready = purlis_core::start::Ready {
            harness: Some(Harness::ClaudeCode),
            unsandboxed: Some(a_person_lifted_it()),
            ..a_shell_ready()
        };

        let refused = chats
            .start_ready(&chat("/bin/sh", "c", None), &ready, SIZE)
            .expect_err("not started");

        assert!(refused.contains("could not record"), "{refused}");
        assert!(refused.contains("the disk is full"), "{refused}");
        assert!(host.asked().is_empty(), "something ran");
        assert!(lock(&said).is_empty(), "{:?}", lock(&said));
        assert!(chats.record().chats.is_empty());
    }

    #[test]
    fn a_persons_opt_out_with_no_event_log_at_all_is_not_started() {
        let host = Pretend::default();
        let chats = Chats::on_host(Box::new(|_| {}), Box::new(host.clone()));
        let ready = purlis_core::start::Ready {
            harness: Some(Harness::ClaudeCode),
            unsandboxed: Some(a_person_lifted_it()),
            ..a_shell_ready()
        };

        let refused = chats
            .start_ready(&chat("/bin/sh", "c", None), &ready, SIZE)
            .expect_err("not started");

        assert!(refused.contains("event log is not open"), "{refused}");
        assert!(host.asked().is_empty(), "something ran");
    }

    /// A system with no backend starts every chat without the sandbox, so a missing record
    /// cannot refuse them all: the chat starts, and its tab says the record is missing.
    #[test]
    fn a_windows_start_that_cannot_be_recorded_still_starts_and_its_tab_says_so() {
        let host = Pretend::default();
        let mut chats = Chats::on_host(Box::new(|_| {}), Box::new(host.clone()));
        let _said = saying_or(&mut chats, Some("the disk is full"));
        let ready = purlis_core::start::Ready {
            harness: Some(Harness::ClaudeCode),
            unsandboxed: Some(purlis_core::sandbox::Lifted {
                by: purlis_core::sandbox::By::NoBackend(purlis_core::sandbox::Os::Windows),
                reason: None,
            }),
            ..a_shell_ready()
        };

        let session = chats
            .start_ready(&chat("/bin/sh", "c", None), &ready, SIZE)
            .expect("starts");

        let notes = chats.start_notes(session);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("could not record"), "{notes:?}");
        assert!(notes[0].contains("the disk is full"), "{notes:?}");
        assert!(chats.start_notes(session).is_empty(), "said once");
    }

    #[test]
    fn only_a_shell_tab_at_the_project_root_is_a_place_to_type_an_install_command() {
        let root = tempfile::tempdir().expect("a project");
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let chats = Chats::on_host(Box::new(|_| {}), Box::new(Pretend::default()));
        let shell_at = |cwd: &std::path::Path| {
            chats
                .start(
                    &Chat {
                        cwd: Some(cwd.to_path_buf()),
                        ..chat("/bin/sh", "shell", None)
                    },
                    SIZE,
                )
                .expect("starts")
        };
        let here = shell_at(root.path());
        let there = shell_at(elsewhere.path());
        let agent = chats
            .start_ready(
                &Chat {
                    cwd: Some(root.path().to_path_buf()),
                    ..chat("/bin/sh", "c", None)
                },
                &purlis_core::start::Ready {
                    harness: Some(Harness::ClaudeCode),
                    ..a_shell_ready()
                },
                SIZE,
            )
            .expect("starts");

        assert!(chats.is_shell_at(here, root.path()));
        assert!(!chats.is_shell_at(there, root.path()), "another directory");
        assert!(!chats.is_shell_at(agent, root.path()), "an agent's pane");
        assert!(!chats.is_shell_at(9999, root.path()), "no such session");
    }
}
