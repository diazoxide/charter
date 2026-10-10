//! **System notifications, for asks only** (#1694, spec #1688, I-7).
//!
//! What the person is interrupted for is what blocks work: an **ask** (a permission prompt, a
//! dispatch grant, a sandbox host, a prompt in a harness's terminal, a chat whose turn ended
//! on them). An **update** — a task that failed, a refused commit, a report — is information,
//! and never raises a notification ([`an_update`]).
//!
//! **What is asked is the asks registry's** (`asking::every_ask`), read again whenever one of
//! its sources may have moved ([`poke`]): a chat moving on the board, its hooks' asks, a
//! dispatch held, a block heard — the events the window reads its own list again on. An ask
//! is notified once, the first time it is read ([`Rules`]); it holds no store of its own past
//! the keys it has seen, so it agrees with the Inbox by construction. A chat that waits on its
//! background agents is not in the queue (#1626), so it is no ask and sends nothing. A chat's
//! own wait is keyed by the chat, so the board's edge into the queue renews it ([`moved`]).
//!
//! **A notification says who asks and what kind of ask it is** (`steward 12 › #3046 drill`,
//! "Asks your permission"), never the ask's words ([`kind_said`]).
//!
//! **One chat's asks a few seconds apart share one notification** ([`TOGETHER`]): the first
//! is sent at once, and each that follows within the window of the one before rides it. Two
//! chats are two notifications.
//!
//! **Nothing is sent while the person is looking** ([`Looking`]): the window holding the
//! ask's project is on screen, focused and has that project in front, and it shows either the
//! chat itself (`Chats::looks_at`, #1486, #1489) or that project's Inbox ([`InboxOpen`], which
//! the window says). Every question unanswered reads as "not looking", so a notification is
//! sent rather than held back: one not needed costs a glance, one needed and not sent costs a
//! chat sitting unanswered.
//!
//! **A click opens the Inbox at that group.** The desktop tells the app nothing of a click but
//! that it is brought forward, so the window holding the project is sent [`NOTIFIED`] with
//! the chat, and lands there the next time it comes to the front (`askNotices.ts`).

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tauri::Manager;

use crate::asking::{AskSource, Shown};
use crate::planes::{PlaneId, Planes, Showing};

/// The event the window holding a project is sent when a notification about one of its chats
/// went out: [`Landing`].
pub const NOTIFIED: &str = "asks-notified";

/// How close together one chat's asks are to share one notification: each within this of the
/// one before rides the first.
pub const TOGETHER: Duration = Duration::from_secs(5);

/// How long a burst of moves is let settle before the registry is read, as the window does.
const SETTLE: Duration = Duration::from_millis(150);

/// One notification to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub plane: PlaneId,
    /// The chat whose asks it is about: the Inbox's group a click lands on.
    pub session: u32,
    /// Who is asking, as its chain (I-9).
    pub title: String,
    /// What kind of ask it is ([`kind_said`]), and how many more ride with it.
    pub body: String,
}

/// Where a click on a notification lands, as the window is sent it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Landing {
    pub plane: PlaneId,
    pub session: u32,
}

/// One read of a project's asks, as [`Rules::heard`] takes it.
pub struct Heard<'a> {
    pub plane: &'a PlaneId,
    /// The registry's whole list, as it was derived.
    pub asks: &'a [Shown],
    /// Whether an item of the list is an update rather than an ask ([`an_update`]).
    pub update: &'a dyn Fn(&Shown) -> bool,
    /// Whether the person is looking at a chat's asks now ([`Looking`]).
    pub looking: &'a dyn Fn(u32) -> bool,
    pub now: Instant,
}

/// **What has been told, and when each chat was last**: the whole of the notifier's memory.
#[derive(Default)]
pub struct Rules {
    /// The asks of each project read last, by key, with the chat that asks and whether the
    /// ask is the chat's own wait ([`own_wait`]): an ask in here was already considered.
    known: HashMap<PlaneId, HashMap<String, (u32, bool)>>,
    /// When each chat last had an ask that a notification covered.
    covered: HashMap<(PlaneId, u32), Instant>,
}

impl Rules {
    /// The notifications one read of a project's asks sends: one per chat with an ask not
    /// read before, unless the chat's asks are riding one sent within [`TOGETHER`], or the
    /// person is looking at them. An ask the person was looking at is not sent later either.
    pub fn heard(&mut self, heard: &Heard<'_>) -> Vec<Notice> {
        let asks: Vec<&Shown> = heard
            .asks
            .iter()
            .filter(|ask| !(heard.update)(ask))
            .collect();
        let known = self.known.entry(heard.plane.clone()).or_default();
        let mut fresh: Vec<(u32, Vec<&Shown>)> = Vec::new();
        for ask in asks.iter().filter(|ask| !known.contains_key(&ask.ask)) {
            match fresh
                .iter_mut()
                .find(|(session, _)| *session == ask.session)
            {
                Some((_, mine)) => mine.push(ask),
                None => fresh.push((ask.session, vec![ask])),
            }
        }
        *known = asks
            .iter()
            .map(|ask| (ask.ask.clone(), (ask.session, own_wait(ask))))
            .collect();
        self.covered
            .retain(|_, at| heard.now.saturating_duration_since(*at) < TOGETHER);
        let mut sent = Vec::new();
        for (session, mine) in fresh {
            let key = (heard.plane.clone(), session);
            if let Some(at) = self.covered.get_mut(&key) {
                *at = heard.now;
                continue;
            }
            if (heard.looking)(session) {
                continue;
            }
            self.covered.insert(key, heard.now);
            sent.push(notice(heard.plane, session, &mine));
        }
        sent
    }

    /// **Chat `session` of `plane` waits on the person anew**, as the board's own edge says
    /// (`Moved::interrupts`): its own wait, a reply or a prompt in its terminal, is keyed by
    /// the chat and not by the wait, so a turn that ended on the person again between two
    /// reads would read as the same ask. It is fresh at the next read. A decision it holds,
    /// keyed by itself, is untouched.
    pub fn renew(&mut self, plane: &PlaneId, session: u32) {
        if let Some(known) = self.known.get_mut(plane) {
            known.retain(|_, (asker, own)| !(*own && *asker == session));
        }
    }

    /// A project let go of, or no longer held: nothing of it is remembered.
    pub fn forget(&mut self, plane: &PlaneId) {
        self.known.remove(plane);
        self.covered.retain(|(held, _), _| held != plane);
    }
}

/// **What a notification says an ask is**: its kind, never its words. A command line, a host
/// or a path is the window's to show; a notification is drawn where the window is not, on a
/// locked screen and in the system's own list of them, and kept there.
fn kind_said(source: AskSource) -> &'static str {
    match source {
        AskSource::Permission => "Asks your permission",
        AskSource::Dispatch => "Asks to hand a task on",
        AskSource::SandboxHost => "Asks to reach a host",
        AskSource::Terminal => "Waiting in its terminal",
        AskSource::Question => "Waiting on your reply",
    }
}

/// Whether `ask` is a chat's own wait, keyed by the chat: a reply it waits on, or a prompt in
/// its terminal.
fn own_wait(ask: &Shown) -> bool {
    matches!(ask.source, AskSource::Question | AskSource::Terminal)
}

/// The notification for chat `session`'s new asks, `asks`, the first first.
fn notice(plane: &PlaneId, session: u32, asks: &[&Shown]) -> Notice {
    let first = asks[0];
    let title = if first.chain.is_empty() {
        format!("chat {session}")
    } else {
        first.chain.join(" › ")
    };
    let kind = kind_said(first.source);
    let body = match asks.len() {
        1 => kind.to_owned(),
        many => format!("{kind}, and {} more", many - 1),
    };
    Notice {
        plane: plane.clone(),
        session,
        title,
        body,
    }
}

/// **Whether an item of the registry is an update, not an ask**: a chat in the needs-you queue
/// for `reasons` of the app's own — a report with nowhere to go, a commit refused
/// ([`reasons_besides_failures`]) — and not for anything it asked. The Inbox says such a
/// chat's reason in place of a reply box (#1692). Every other source is a decision the person
/// owes, whatever else the chat waits for.
pub fn an_update(ask: &Shown, reasons: usize) -> bool {
    ask.source == AskSource::Question && reasons > 0
}

/// The reasons of the app's own chat `session` is in the queue for, as the board says them.
fn app_reasons(board: &dyn crate::host::ChatBoard, session: u32) -> usize {
    let now = board.now(session);
    reasons_besides_failures(now.needs.as_deref(), now.refusals.len())
}

/// **The app's reasons that make a chat's wait an update**: a report with nowhere to go and a
/// refused commit. Not a task of its that came to nothing (#1693): a chat in the queue only for
/// those asks nothing and is no ask at all (`asking::derive`), and one that also waits on the
/// person for itself still asks, so a failure left unlooked-at never silences its notification.
pub fn reasons_besides_failures(needs: Option<&[crate::hooks::Need]>, refusals: usize) -> usize {
    needs.map_or(0, |needs| {
        needs
            .iter()
            .filter(|need| !matches!(need, crate::hooks::Need::TaskFailed { .. }))
            .count()
    }) + refusals
}

/// **Whether the person is looking at a chat's asks**: all of the window's questions, each
/// answered by the window holding the chat's project.
#[derive(Debug, Clone, Copy)]
pub struct Looking {
    /// That window is on screen, has the keyboard and has the project in front.
    pub window_in_front: bool,
    /// It has the project's Inbox open.
    pub inbox_open: bool,
    /// It has the chat on screen in the tab in front.
    pub chat_on_screen: bool,
}

impl Looking {
    pub fn is_looking(self) -> bool {
        self.window_in_front && (self.inbox_open || self.chat_on_screen)
    }
}

/// **Which project's Inbox each window has open**, as the window says it (`inbox_shown`).
/// A window that never said has none open.
#[derive(Default)]
pub struct InboxOpen {
    open: Mutex<HashSet<(String, PlaneId)>>,
}

impl InboxOpen {
    fn open(&self) -> MutexGuard<'_, HashSet<(String, PlaneId)>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `window` says whether it has `plane`'s Inbox open.
    pub fn shown(&self, window: &str, plane: &PlaneId, open: bool) {
        let key = (window.to_owned(), plane.clone());
        if open {
            self.open().insert(key);
        } else {
            self.open().remove(&key);
        }
    }

    pub fn is_open(&self, window: &str, plane: &PlaneId) -> bool {
        self.open().contains(&(window.to_owned(), plane.clone()))
    }

    /// A window gone has no Inbox open.
    pub fn forget_window(&self, window: &str) {
        self.open().retain(|(label, _)| label != window);
    }
}

/// **The notifier**: where the registry's sources say a project's asks may have moved, and
/// the one thread that reads them and sends.
pub struct Notifier {
    poked: Mutex<mpsc::Sender<Poke>>,
    inbox: InboxOpen,
}

impl Notifier {
    /// Starts the thread that reads and sends, for `app`.
    pub fn start(app: &tauri::AppHandle) -> Self {
        let (poked, heard) = mpsc::channel();
        let app = app.clone();
        let _ = std::thread::Builder::new()
            .name("purlis-ask-notifications".to_owned())
            .spawn(move || listen(&app, &heard));
        Self {
            poked: Mutex::new(poked),
            inbox: InboxOpen::default(),
        }
    }
}

/// What the notifier is told: a project whose asks may have moved, and the chat that waits on
/// the person anew, where the board said one does.
type Poke = (PlaneId, Option<u32>);

/// `plane`'s asks may have moved: they are read again once the burst settles.
pub fn poke(app: &tauri::AppHandle, plane: &PlaneId) {
    tell(app, (plane.clone(), None));
}

/// Chat `moved` moved: its project's asks are read again, and a chat that came to wait on the
/// person by its own move (`Moved::interrupts`) waits anew ([`Rules::renew`]).
pub fn moved(app: &tauri::AppHandle, moved: &crate::hooks::Moved) {
    tell(
        app,
        (
            moved.plane.clone(),
            moved.interrupts().then_some(moved.session),
        ),
    );
}

fn tell(app: &tauri::AppHandle, poke: Poke) {
    if let Some(notifier) = app.try_state::<Notifier>() {
        let poked = notifier
            .poked
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let _ = poked.send(poke);
    }
}

/// A window gone: it has no Inbox open.
pub fn window_gone<R: tauri::Runtime>(app: &impl Manager<R>, window: &str) {
    if let Some(notifier) = app.try_state::<Notifier>() {
        notifier.inbox.forget_window(window);
    }
}

/// The window says whether it has `plane`'s Inbox open (#1694): while it is, and the window
/// is focused with the project in front, no notification is sent about that project's asks.
#[tauri::command]
#[specta::specta]
pub fn inbox_shown(
    window: tauri::Window,
    notifier: tauri::State<'_, Notifier>,
    plane: PlaneId,
    open: bool,
) {
    notifier.inbox.shown(window.label(), &plane, open);
}

/// The notifier's thread: each poke, then whatever follows it while it settles, read once.
fn listen(app: &tauri::AppHandle, heard: &mpsc::Receiver<Poke>) {
    let mut rules = Rules::default();
    while let Ok(first) = heard.recv() {
        let mut pokes = vec![first];
        let until = Instant::now() + SETTLE;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match heard.recv_timeout(left) {
                Ok(poke) => pokes.push(poke),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let mut planes: Vec<PlaneId> = Vec::new();
        for (plane, anew) in pokes {
            if let Some(session) = anew {
                rules.renew(&plane, session);
            }
            if !planes.contains(&plane) {
                planes.push(plane);
            }
        }
        for plane in planes {
            read(app, &mut rules, &plane);
        }
    }
}

/// Reads `plane`'s asks and sends what [`Rules`] says to.
fn read(app: &tauri::AppHandle, rules: &mut Rules, plane: &PlaneId) {
    let Some(held) = app
        .try_state::<Planes>()
        .and_then(|planes| planes.held(plane).ok())
    else {
        rules.forget(plane);
        return;
    };
    let asking = crate::asking::every_ask(&held);
    let board = held.board();
    let sent = rules.heard(&Heard {
        plane,
        asks: &asking.asks,
        // The board is asked only about a chat waiting on a reply: no other source can be one.
        update: &|ask| {
            ask.source == AskSource::Question && an_update(ask, app_reasons(board, ask.session))
        },
        looking: &|session| looking(app, plane, session, &held).is_looking(),
        now: Instant::now(),
    });
    for notice in sent {
        send(app, notice);
    }
}

/// What the window holding `plane` answers about chat `session` ([`Looking`]).
///
/// **The window asked is the one holding the chat's project** (charter#126): with a project
/// split into a window of its own, asking the main window would hold a notification back
/// because the person was looking at a different window. **And that window must have the
/// project in front** (#111): every project numbers its chats from one, so "is chat 3 on
/// screen" has as many answers as there are projects open, and only the window knows which one
/// it draws. A task shown inside its session's tab is on screen; a session's own chat hidden
/// behind a task is not (`Chats::looked_at`).
fn looking(
    app: &tauri::AppHandle,
    plane: &PlaneId,
    session: u32,
    held: &crate::planes::Held,
) -> Looking {
    let showing = app.try_state::<Showing>();
    let window = showing
        .as_ref()
        .and_then(|showing| showing.holder(plane))
        .and_then(|label| app.get_webview_window(&label));
    let Some(window) = window else {
        return Looking {
            window_in_front: false,
            inbox_open: false,
            chat_on_screen: false,
        };
    };
    let window_in_front = window.is_visible().unwrap_or(false)
        && window.is_focused().unwrap_or(false)
        && showing.is_some_and(|showing| showing.is_showing(window.label(), plane));
    Looking {
        window_in_front,
        inbox_open: app
            .try_state::<Notifier>()
            .is_some_and(|notifier| notifier.inbox.is_open(window.label(), plane)),
        chat_on_screen: held.chats().looks_at(session),
    }
}

/// Sends one notification, and tells the window holding its project where a click lands.
fn send(app: &tauri::AppHandle, notice: Notice) {
    use tauri_plugin_notification::NotificationExt;
    // Best effort, always. A desktop that refuses notifications, or a person who turned them
    // off, is not a reason for anything else here to stop working.
    let _ = app
        .notification()
        .builder()
        .title(notice.title)
        .body(notice.body)
        .show();
    let landing = Landing {
        plane: notice.plane,
        session: notice.session,
    };
    crate::windows::emit_for_plane(app, &landing.plane.clone(), NOTIFIED, &landing);
}

#[cfg(test)]
#[path = "asknotify_tests.rs"]
mod tests;
