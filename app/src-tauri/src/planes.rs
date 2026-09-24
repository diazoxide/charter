//! Every plane this process holds open, and what it holds for each one.
//!
//! **The app used to hold exactly one.** `Hooks`, `Chats` and the plane's root were managed
//! as process-wide singletons, every command took `State<'_, Hooks>` or `State<'_, Chats>`,
//! and which plane those belonged to was whatever directory the process was launched from.
//! A project IS a plane, so an app that can only hold one can only ever show one project.
//!
//! What replaces it is a registry keyed by the plane's root. Nothing per-plane is managed by
//! Tauri any more, so there is no `State<'_, Chats>` to reach for: the only way to a board or
//! a chat is [`Planes::held`], and that takes the [`PlaneId`] the caller is acting for. A
//! command that forgets which plane it means does not compile.
//!
//! **A session number means nothing without its plane.** Each plane numbers its own chats
//! from one, and that number is what charter hands the chat as `CHARTER_SESSION_ID` — the
//! rung `.charter/sessions/<sid>.workspace` is keyed on, inside that plane. Numbering across
//! planes instead would make one plane's chat numbers depend on which other planes the
//! process happened to be holding, and put that dependency on disk. So the identity of a chat
//! is the pair, and every command that names a session names its plane beside it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use charter_core::engine::Size;
use charter_core::machine;
use charter_core::reopen;
use charter_core::reopen::Choice;

use crate::chats::Chats;
use crate::hooks::{self, Hooks, Moved};
use crate::sessions::Reporting;

/// Which plane something is acting for.
///
/// It is the plane's root as the registry resolved it, and it is minted by [`Planes::open`]
/// alone — a caller hands one back, it never spells one. Two spellings of one directory would
/// otherwise be two entries in the registry holding two boards for one plane on disk, which
/// is the "acting on the wrong plane" defect wearing a different hat.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PlaneId(String);

impl PlaneId {
    /// The id of a root that has already been resolved.
    fn of(root: &Path) -> Self {
        Self(root.display().to_string())
    }

    /// The root it names, for a message about it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The operator's yes to acting on one plane's record.
///
/// **`.charter/app/reopen.json` is an execution input.** Putting a record back STARTS the
/// programs it names, and for a chat that was not on a profile what runs is decided from the
/// record alone. A plane is a DIRECTORY, and a directory arrives by zip, by shared folder or
/// on a stick as readily as by `git clone` — more readily, in fact, since `.charter/` is
/// gitignored and does not ride in through a clone at all. So "open this directory" must not
/// be able to mean "run what is written in it".
///
/// It has no constructor outside this module and holds nothing, which is the whole design:
/// [`Planes::open`] attaches a plane and cannot reopen it, and [`Planes::reopen`] is private
/// and takes one of these. A plane that has not been approved cannot reach the record by
/// construction rather than by a caller remembering to look.
///
/// **Two yeses, and they are both written down here.** The launch's own working directory —
/// the operator ran charter there, which is the act — and the operator's answer to the trust
/// ask ([`Planes::approve_and_open`]). Every other way in (an opener handing over a path, a
/// recents row, a second instance's argument) reaches the second of those and nothing else,
/// so adding a third is a deliberate edit to this file rather than an argument somebody
/// forgot to pass.
pub struct Approved(());

/// The one way this app writes a plane's record — **and `reopen::write` is named here and
/// nowhere else in it**, because writing that file and vouching for it are one act.
///
/// charter rewrites `.charter/app/reopen.json` every time a chat opens or closes, and the
/// machine store's trust fingerprint covers the programs that record would start. A
/// fingerprint taken only when the operator approved the plane therefore goes stale on their
/// very next click, and the next launch asks them about a chat they started themselves. An
/// operator trained to dismiss that question is worse off than one who was never asked, so
/// the two writes are not two things a caller has to remember to pair: there is one method,
/// and no path to the first without the second.
///
/// `Store::vouch` never creates an approval, only refreshes one, so a plane nobody has
/// approved stays unapproved however many times charter writes its record.
struct Records {
    root: PathBuf,
    /// Where this machine's store lives, resolved once at startup like the plane itself —
    /// it is an environment ladder, and a second reader of it is a second answer.
    config: Option<PathBuf>,
    /// Whether this plane's record is the app's to read and write.
    ///
    /// Set when the record is put back, which only an approved plane reaches. An attached
    /// plane nobody has approved is not recorded EITHER WAY: reading it would run what it
    /// names, and writing it would replace the operator's own record — of a plane they never
    /// said yes to — with whatever this process happens to have open, which is nothing.
    allowed: AtomicBool,
}

impl Records {
    /// Writes what is open into the plane, then vouches for it.
    ///
    /// **Record first, vouch second, and the order is not a preference.**
    /// `machine::Contribution::of` reads the record back off disk, so a vouch taken before
    /// the write would fingerprint what was there a moment ago — which is the stale
    /// fingerprint this exists to prevent, arrived at from the other side.
    ///
    /// Neither half is worth interrupting the operator over. A record that cannot be written
    /// means the next launch of this plane comes back empty; a vouch that cannot be taken
    /// means one spurious question at that launch. Both are said and neither refuses.
    fn write(&self, record: &reopen::Record) {
        if !self.allowed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(why) = reopen::write(&self.root, record) {
            eprintln!(
                "charter: what is open in {} was not recorded ({why})",
                self.root.display()
            );
            // Not vouched for: the fingerprint would then describe a record charter did not
            // manage to write, and the point of it is that it describes what is there.
            return;
        }
        self.vouch();
    }

    /// Re-fingerprints the plane, because charter itself just changed what opening it would
    /// do.
    ///
    /// A machine with no store — Windows, where `0600` has no expression and the store
    /// refuses outright (ADR 0031, charter-app#98) — simply has nothing to refresh. The app
    /// runs there; it just cannot remember planes between launches.
    fn vouch(&self) {
        let Some(config) = self.config.as_deref() else {
            return;
        };
        let contributed = machine::Contribution::of(&self.root);
        let when = now();
        let root = self.root.clone();
        if let Err(why) = machine::update(config, move |store| {
            store.vouch(&root, contributed, when);
        }) && why.kind() != std::io::ErrorKind::Unsupported
        {
            eprintln!(
                "charter: the record of {} was written but not vouched for ({why}); charter                  may ask about this plane again at the next launch",
                self.root.display()
            );
        }
    }

    /// From here on this plane's record is charter's to write. Nothing before this point
    /// wrote one, so nothing before it vouched for one either.
    fn allow(&self) {
        self.allowed.store(true, Ordering::SeqCst);
    }
}

/// A reopen record as one read of the plane gave it back, or why charter would not read it.
///
/// **A type alias rather than the bare `Result`, so the thing being passed has a name.** What
/// travels from [`Planes::open_if_approved`] and [`Planes::approve_and_open`] down to
/// [`Held::reopen`] is not "a record" — it is *the record this open was decided on*, and the
/// reason it is passed rather than read is charter-app#123. `Reading::record` is where it
/// comes from.
type Read = Result<reopen::Record, std::io::Error>;

/// What the app holds for one open plane: its board, its chats, and where it is.
pub struct Held {
    id: PlaneId,
    root: PathBuf,
    hooks: Hooks,
    /// The window, for a move no hook reports: a chat closed, or a request ignored.
    tell: Teller,
    chats: Chats,
    records: Arc<Records>,
    /// What tells the window the plane moved on disk (charter-app#264). None where the
    /// platform would not watch; the panels then read the plane when focused, as they did.
    watch: Mutex<Option<crate::planewatch::Watch>>,
}

impl Held {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn hooks(&self) -> &Hooks {
        &self.hooks
    }

    pub fn chats(&self) -> &Chats {
        &self.chats
    }

    /// Writes what this plane has open into the plane itself.
    ///
    /// A failure is said and never refused over: the next launch of this plane reads no
    /// record and starts empty, which is worse than a line on standard error and better than
    /// an app that will not close a project.
    fn record(&self) {
        self.records.write(&self.chats.record());
    }

    /// Puts back the chats this plane had open when it was last closed, which STARTS the
    /// programs its record names.
    ///
    /// Said out loud, on standard error, because an operator whose chats did not come back
    /// otherwise has nothing at all to look at — and neither does a CI log.
    ///
    /// The plane starts recording HERE, and not before: from this moment the app has read
    /// what was there, so writing over it is replacing its own answer rather than the
    /// operator's.
    ///
    /// **It does not read the record, and that is charter-app#123's whole fix.** The bytes are
    /// handed in by the caller, which got them from the same read that produced the
    /// contribution the operator was shown (`machine::Contribution::read`). This function used
    /// to open `.charter/app/reopen.json` itself, a moment after `approve_and_open` had
    /// compared what a *different* read of it said — so a write landing between the two was
    /// started without ever having been drawn in a dialog. There is now one read per open, and
    /// no way to write a second one without changing this signature.
    ///
    /// **`choice` is the operator's answer at the launch** (charter-app#250), and every open
    /// that is not one of the launch's projects is [`Choice::ReopenAll`], which is what an open
    /// always did. A fresh start writes the cleared record at once — the choice is the moment
    /// the old one stops being wanted, and a record left as it was would ask again at the next
    /// launch about chats the operator already declined.
    fn reopen(&self, size: Size, record: Read, choice: Choice) {
        self.records.allow();
        let record = match record {
            Ok(record) if choice == Choice::StartFresh => {
                let fresh = record.chosen(Choice::StartFresh);
                self.records.write(&fresh);
                eprintln!(
                    "charter: plane {}, started fresh as asked; nothing is reopened",
                    self.root.display()
                );
                fresh
            }
            Ok(record) => record,
            Err(why) => {
                // Not the same thing as an empty plane, and an operator told "nothing to
                // reopen" would go looking in the wrong place.
                eprintln!(
                    "charter: the record of what was open in {} was refused ({why}); nothing \
                     is reopened and nothing will be recorded until it is repaired",
                    self.root.display()
                );
                reopen::Record::default()
            }
        };
        let wanted = record.chats.len();
        let back = self.chats.put_back(&record, &self.root, size).len();
        if wanted > 0 {
            eprintln!(
                "charter: plane {}, {back} of {wanted} chats back",
                self.root.display()
            );
            for (name, why) in self.chats.would_not_start() {
                eprintln!("charter: {name} did not start ({why}); it is still recorded");
            }
        } else {
            eprintln!("charter: plane {}, nothing to reopen", self.root.display());
        }
    }

    /// Ends a chat and takes it off the board — the tab's ×, or ending its pane — and tells
    /// the window.
    ///
    /// **The telling is the fix for charter-app#247.** The window's needs-you queue is the one
    /// the last `chat-moved` carried, and nothing else ever corrects it. Taking a chat off the
    /// board used to be silent, and the exit that follows a close cannot speak either: by the
    /// time it lands the board no longer has the chat, so `Board::exited` answers "nothing
    /// changed". A chat closed while it was asking for you therefore stayed in the queue, and
    /// in the red counts on its project and workspace tabs, until some other chat moved.
    ///
    /// **Off the board first, and told last.** Off first, so a hook that fires while the
    /// program is being ended finds no chat to move and tells nothing; told last, so a report
    /// the board took just before is told before this, never after it with the chat still
    /// asking. And off and told even when the session had already gone: either way the chat
    /// is gone from the app, and a window left believing otherwise is the defect.
    pub fn close_chat(&self, session: u32) -> Result<(), String> {
        let gone = self.hooks.closed(session);
        let closed = self.chats.close(session);
        (self.tell)(gone);
        closed
    }

    /// Drops a chat's request for the operator without answering it — the needs-you item's
    /// Ignore — and tells the window (charter-app#248).
    ///
    /// **The core holds it, not the window.** The window's queue is the one the last
    /// `chat-moved` carried and it is replaced whole on every move, so an ignore kept in the
    /// window would be undone by the next move of any chat. On the board it lasts exactly as
    /// long as the request does: the chat's next `Stop` or `Notification` asks again.
    ///
    /// The snapshot is built under the board's hold (`Hooks::ignored`), and numbered there,
    /// so a report racing it is put in order by the window rather than by which thread won.
    pub fn ignore_needs_you(&self, session: u32) {
        (self.tell)(self.hooks.ignored(session));
    }

    /// Writes the record, ends every session, and stops listening — everything a plane holds
    /// in this process, and nothing it has on disk.
    ///
    /// The record is written BEFORE the sessions are ended, because ending them is what makes
    /// there be nothing to write.
    fn let_go(&self) {
        self.record();
        self.chats.end_all();
        self.hooks.stop();
        drop(
            self.watch
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
    }
}

/// Told whenever a chat moves, whichever plane it is in. The event carries its plane, so one
/// teller serves them all.
pub type Teller = Arc<dyn Fn(Moved) + Send + Sync + 'static>;

/// The planes this process holds, by root.
pub struct Planes {
    tell: Teller,
    /// The `charter` binary a hook runs and the plugin a chat loads, where the app found them.
    /// Every plane arms with the same ones: they are a property of this build, not of a project.
    shipped: crate::Shipped,
    /// Where this machine's store lives, or none on a machine with no config home at all.
    config: Option<PathBuf>,
    open: Mutex<HashMap<PlaneId, Arc<Held>>>,
    /// Told when a handoff has opened a chat in any plane (charter-app#204).
    arrivals: crate::handoff::Arrivals,
    /// Told when any plane changes on disk (charter-app#264).
    changes: crate::planewatch::Changed,
    /// The launch's question and its answer — see [`Relaunching`].
    relaunching: Mutex<Relaunching>,
}

/// Where the launch's question stands (charter-app#250): what it holds back until the
/// operator answers, and what the answer was.
///
/// **Nothing the launch would put back starts before the answer.** The launch's own plane is
/// attached at once — its board, its socket, its tab — but its record waits here as `owed`,
/// and the window restores the other projects only after it has answered. The answer is taken
/// once: a webview that reloads asks again, and a second put-back would be a second copy of
/// every chat.
#[derive(Default)]
struct Relaunching {
    /// The plane the launch's working directory named, attached and not yet put back.
    owed: Option<PlaneId>,
    /// Whether the operator has answered. From then on there is no question to ask.
    decided: bool,
    /// The roots the answer said to start fresh, each taken the first time it is put back.
    /// Empty after "Reopen all", which is what every open did before there was a question.
    fresh: HashSet<PathBuf>,
}

/// What a launch would put back, for the question it asks before putting any of it back.
#[derive(Debug)]
pub struct Relaunchable {
    /// Every project with a chat or a view tab to put back, the launch's own first.
    pub projects: Vec<Waiting>,
    /// Whether any of their records was written by a quit that restarted charter to install an
    /// update (charter-app#251), which changes what the question says.
    pub after_update: bool,
}

/// One project's share of [`Relaunchable`].
#[derive(Debug)]
pub struct Waiting {
    pub root: PathBuf,
    pub chats: usize,
    pub views: usize,
}

impl Planes {
    /// A registry holding nothing, which is what the app comes up as before a plane is
    /// opened — and stays as, perfectly happily, when there is no plane to open.
    pub fn telling(tell: Teller, shipped: crate::Shipped, config: Option<PathBuf>) -> Self {
        Self {
            tell,
            shipped,
            config,
            open: Mutex::new(HashMap::new()),
            // Nobody to tell yet. A registry with no window still opens a handed-off chat;
            // it simply has no strip to put it on until one asks what is open.
            arrivals: Arc::new(|_| {}),
            changes: Arc::new(|_| {}),
            relaunching: Mutex::new(Relaunching::default()),
        }
    }

    /// Tells `arrivals` whenever a handoff opens a chat, so the window can put it on a strip.
    pub fn telling_arrivals(mut self, arrivals: crate::handoff::Arrivals) -> Self {
        self.arrivals = arrivals;
        self
    }

    /// Tells `changes` whenever a plane this registry holds changes on disk, so the window reads
    /// it again (`crate::planewatch`).
    pub fn telling_changes(mut self, changes: crate::planewatch::Changed) -> Self {
        self.changes = changes;
        self
    }

    /// Opens `root`: binds its hook socket and arms its board. Answers with the id every
    /// later call names it by.
    ///
    /// **Its record is not touched.** Putting one back starts programs, so it needs the
    /// operator's yes — see [`Approved`] and [`Planes::reopen`]. An opened plane with no yes
    /// behind it is a live, empty project: chats can be started in it, and nothing that was
    /// written in the directory runs.
    ///
    /// **A root already open is answered with the id it already has, and nothing is bound a
    /// second time.** `Listener::bind` removes a socket left behind by a process that is
    /// gone, which is right for a stale one and would be a disaster for a live one: the
    /// sessions of the plane already open carry that path in their environment, so their
    /// hooks would report onto a board belonging to a second `Chats` that numbers its chats
    /// from one. Every state would land on the wrong chat.
    pub fn open(&self, root: &Path) -> PlaneId {
        // Resolved once, here. Everything below — the socket, the record, the registry key —
        // is this one spelling of the plane, so nothing downstream has to resolve anything.
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        // **The app's own line of the fence** (charter-app#129). Every per-plane thing this
        // app holds — the board, the chats, the hook socket in `.charter/app/`, the machine
        // store entry made a line below — hangs off this call and off no other, because a
        // `PlaneId` is minted here alone. So a fenced build that must not touch a plane has
        // exactly one place to say so, and a caller that reached a root some other way than
        // `plane::resolve` is held all the same.
        charter_core::fence::hold(charter_core::fence::Act::Open, &root);
        let id = PlaneId::of(&root);
        // Remembered HERE, and therefore under the spelling the line above settled on. Done
        // in the caller instead, it would be done against whatever path that caller happened
        // to hold — and a machine store keyed on `/var/…` while every approval is keyed on
        // `/private/var/…` is two entries for one project, each answering half the question.
        // It is not an approval: `Store::remember` carries an existing one over and creates
        // none, so opening a plane a hundred times does not become consent to it.
        self.remember(&root);

        let mut open = self.map();
        if let Some(already) = open.get(&id) {
            return already.id.clone();
        }
        let held = Arc::new(self.hold(id.clone(), root));
        // A handoff from one of this plane's chats is answered by this plane, which is the
        // only one holding the asking chat's record. A `Weak`, because the plane holds the
        // socket that holds this answer: a strong handle would keep a closed plane alive.
        held.hooks().answer_with({
            let held = Arc::downgrade(&held);
            let plane = id.clone();
            let tickets = charter_core::hookwire::Tickets::default();
            let arrivals = Arc::clone(&self.arrivals);
            Arc::new(move |connection, ask| match held.upgrade() {
                Some(held) => {
                    crate::handoff::answer(&held, &plane, &tickets, connection, ask, &*arrivals)
                }
                None => charter_core::hookwire::Answer::No {
                    why: "this project has been closed".to_owned(),
                },
            })
        });
        open.insert(id.clone(), Arc::clone(&held));
        id
    }

    /// The record of a plane this registry is already holding, read against the root the
    /// registry settled on.
    ///
    /// **For the launch, which has no dialog to have shown one.** Every other caller of
    /// [`Self::reopen`] arrives with the record already in hand, from the same read that
    /// produced the contribution the operator approved (charter-app#123). A plane the registry
    /// is not holding has no record here rather than a guessed path: the id is a `display()`
    /// of the root, and spelling it back into a path substitutes for bytes that are not UTF-8.
    fn record_of(&self, plane: &PlaneId) -> Read {
        match self.held(plane) {
            Ok(held) => reopen::read_or_refusal(&held.root),
            Err(why) => Err(std::io::Error::other(why)),
        }
    }

    /// Puts back what the plane held, which **starts the programs its record names**.
    ///
    /// Private, and it takes [`Approved`] — a value nothing outside this module can build.
    /// That is the gate: [`Planes::open`] above attaches a plane and has no way to reach
    /// this, so a plane the operator has not said yes to cannot run anything.
    ///
    /// `record` is the bytes the caller decided on, never a path for this to read: see
    /// [`Held::reopen`] and charter-app#123.
    fn reopen(&self, plane: &PlaneId, _yes: &Approved, record: Read) {
        // The handle is taken and the lock dropped BEFORE anything starts: reopening runs
        // programs, and a program that dies at once tells the board, which tells the window,
        // which asks this very registry what the chat is called.
        let Ok(held) = self.held(plane) else { return };
        // A project the launch's answer said to start fresh is started fresh the first time it
        // is put back, and only that time. That first time may be later in the day — a restore
        // whose trust ask was declined, opened afterwards from the recents — which is still
        // the operator's answer for that project; after it, an open is an ordinary one.
        let choice = if self.relaunching().fresh.remove(held.root()) {
            Choice::StartFresh
        } else {
            Choice::ReopenAll
        };
        held.reopen(STARTING, record, choice);
    }

    /// What this launch would put back, or nothing when there is nothing to ask about — no
    /// project holds a chat or a view tab — or the operator has already answered.
    ///
    /// `restoring` is the projects the window is about to restore (`opener::planes_to_restore`),
    /// named by the paths the window will open them by. **Reading a record here starts
    /// nothing**: the counts are all this takes from it, and each project is read again, by the
    /// one read its own open makes, when it is actually put back (charter-app#123).
    pub fn relaunch_ask(&self, restoring: &[PathBuf]) -> Option<Relaunchable> {
        let relaunching = self.relaunching();
        if relaunching.decided {
            return None;
        }
        let roots = self.launch_roots(&relaunching, restoring);
        drop(relaunching);
        let mut after_update = false;
        let projects: Vec<Waiting> = roots
            .into_iter()
            .filter_map(|root| {
                // A record charter refuses to read puts nothing back, so it is not asked about;
                // the refusal is said where it always was, when the project is opened.
                let record = reopen::read_or_refusal(&root).ok()?;
                after_update |= record.relaunch_after_update;
                record.holds_anything().then_some(Waiting {
                    chats: record.chats.len(),
                    views: record.views.len(),
                    root,
                })
            })
            .collect();
        (!projects.is_empty()).then_some(Relaunchable {
            projects,
            after_update,
        })
    }

    /// The operator's answer to [`Self::relaunch_ask`] — or the answer a launch with nothing
    /// to ask about takes without asking, which is [`Choice::ReopenAll`].
    ///
    /// **Taken once.** A second answer — a reloaded window — changes nothing.
    ///
    /// Puts the launch's own plane back, which it has owed since [`at_launch`] attached it,
    /// and marks every project in `restoring` for a fresh start when that is the answer, so
    /// the window's restore opens them through the ordinary gate and they come back empty.
    ///
    /// **This is where the launch's yes — the first of the two approvals [`Approved`] names —
    /// is spent now**, and it covers the one plane [`at_launch`] wrote down as owed, never a
    /// plane the caller names.
    pub fn relaunch(&self, choice: Choice, restoring: &[PathBuf]) {
        let owed = {
            let mut relaunching = self.relaunching();
            if relaunching.decided {
                return;
            }
            relaunching.decided = true;
            if choice == Choice::StartFresh {
                relaunching.fresh = self
                    .launch_roots(&relaunching, restoring)
                    .into_iter()
                    .collect();
            }
            relaunching.owed.take()
        };
        // The lock is dropped before anything starts, for `reopen`'s reason: a program that
        // dies at once tells the window, which asks this registry about it.
        if let Some(plane) = owed {
            self.reopen(&plane, &Approved(()), self.record_of(&plane));
        }
    }

    /// Every project this launch puts back, by root: its own plane first, then each project
    /// the window will restore. The one list both the question and the answer are about, so
    /// what is asked about and what is started fresh cannot drift apart.
    fn launch_roots(&self, relaunching: &Relaunching, restoring: &[PathBuf]) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = relaunching
            .owed
            .as_ref()
            .and_then(|plane| self.held(plane).ok())
            .map(|held| held.root.clone())
            .into_iter()
            .collect();
        for path in restoring {
            // A project that is no longer a plane is the restore's own news, and it says so.
            if let Ok(root) = plane_at(path)
                && !roots.contains(&root)
            {
                roots.push(root);
            }
        }
        roots
    }

    fn relaunching(&self) -> MutexGuard<'_, Relaunching> {
        self.relaunching
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens a plane the operator asked for — **only if this machine has already recorded
    /// their yes to exactly what it contributes now.**
    ///
    /// The consent is read HERE, from the store, against the plane as it is on this disk at
    /// this instant, and the answer decides between the two arms. A caller cannot open the
    /// plane and then think about trust, because there is no way to open it that does not
    /// come through this function or [`Self::approve_and_open`], and both end in the one
    /// private mint below.
    ///
    /// **An ask attaches nothing.** [`Self::open`] is safe to call on an unapproved plane —
    /// it binds a socket and starts no program — but an operator who cancels the dialog would
    /// be left with a live, empty project in the registry that they never asked for, and a
    /// hook socket bound in a stranger's directory. So the ask arm touches the plane only to
    /// read what it would contribute.
    pub fn open_if_approved(&self, root: &Path) -> Result<Opening, String> {
        let root = plane_at(root)?;
        // **A plane this process is already holding is answered, not asked about.** "Open it"
        // then means "show me that project", which is what a recents row and a second launch
        // both mean when they name a plane already on screen. Asking again would put a dialog
        // in front of chats that are already running, about a grant that is already in force,
        // and answering it would reach [`Self::minted`] — see the guard there for what that
        // would have cost.
        let id = PlaneId::of(&root);
        if self.held(&id).is_ok() {
            self.remember(&root);
            return Ok(Opening::Open(id));
        }
        // ONE read, and both arms are cut from it (charter-app#123). The ask arm throws the
        // record away on purpose: nothing is started, and by the time the operator clicks,
        // `approve_and_open` reads again to compare what they read against what is there now.
        // The open arm keeps it, because it is about to run it.
        let reading = machine::Contribution::read(&root);
        let consent = self.consent_to(&root, &reading.contributes);
        if consent.must_ask() {
            return Ok(Opening::Ask(Asking {
                root,
                contributes: reading.contributes,
                consent,
            }));
        }
        Ok(Opening::Open(self.minted(&root, reading.record)))
    }

    /// The operator's yes to a plane, and the open it authorises.
    ///
    /// **The click IS the consent; the store is where it is REMEMBERED.** The difference
    /// matters on a machine that has no store at all — Windows, where `0600` has no
    /// expression and `machine` refuses outright (ADR 0031, charter-app#98). There the answer
    /// cannot be written down, so charter asks again at the next launch and opens on this
    /// one. A gate that refused to open anything where it could not file the answer would not
    /// be a gate; it would be the app turned off.
    ///
    /// **`shown` is the contribution the dialog drew, and it is checked against the plane
    /// again here.** Between the ask and the click, anything on the machine — including a
    /// chat running in another plane — can rewrite this plane's `.claude/settings.json` or
    /// its reopen record. Without this check the approval would record whatever was on disk
    /// at CLICK time, so the operator could approve, and charter could then start, a program
    /// they never read. `approve_profile` makes exactly this check about a profile's command
    /// line, for exactly this reason, and a review probe is what found it missing there.
    ///
    /// **And the record compared here is the record that runs** (charter-app#123). The check
    /// above used to be made against one read of `.charter/app/reopen.json` and `put_back`
    /// then made its own, so a write landing in between was executed without having been
    /// shown. `Contribution::read` hands back the bytes it judged and they travel down to
    /// `put_back` unchanged, which closes that window by construction rather than by narrowing
    /// it.
    ///
    /// What that does **not** close is ADR 0035's own stated limit: the fingerprint is *"not a
    /// defence against an agent that set out to forge the fingerprint"*. A chat that can write
    /// this plane can write it before the dialog is drawn just as easily as after. What closes
    /// is the gap between a human clicking a button and the app reading a file.
    pub fn approve_and_open(
        &self,
        root: &Path,
        shown: &machine::Contribution,
    ) -> Result<PlaneId, String> {
        self.approving(root, shown, || {})
    }

    /// [`Self::approve_and_open`], with a seam between the read and the open.
    ///
    /// `between` exists for one test and nothing else: charter-app#123 is a window, and a
    /// window can only be shown to have closed by a writer that lands *in* it. A test that
    /// writes before the call is testing the comparison above, which was already there; a test
    /// that writes after it is testing nothing at all. This is the same seam, for the same
    /// reason, that `machine::write_through` keeps so a test can plant its link at the path
    /// that is actually opened.
    fn approving(
        &self,
        root: &Path,
        shown: &machine::Contribution,
        between: impl FnOnce(),
    ) -> Result<PlaneId, String> {
        let root = plane_at(root)?;
        let reading = machine::Contribution::read(&root);
        if &reading.contributes != shown {
            return Err(format!(
                "{} changed while you were reading it, so nothing was approved and nothing was \
                 opened. Open it again to see what it contributes now.",
                charter_core::shown::short(&root.display().to_string())
            ));
        }
        between();
        self.record_approval(&root, reading.contributes);
        Ok(self.minted(&root, reading.record))
    }

    /// Attaches the plane, remembers that it was opened, and puts its record back.
    ///
    /// **The one place the operator-facing [`Approved`] is minted**, reached from exactly two
    /// callers: a consent the store had already recorded, and the operator's own click.
    ///
    /// [`Planes::open`] remembers the plane on the way through, and it has to happen before
    /// the record is put back: [`Records::write`] vouches for the record it just wrote, and
    /// `Store::vouch` refreshes an entry rather than creating one — so a plane not yet in the
    /// list would be written and then not vouched for, and the very next launch would ask
    /// about a record charter itself had written.
    /// **And the record is put back once per open, never once per yes.** `Planes::open` hands
    /// back the id a plane already has and binds nothing a second time; `reopen` has no such
    /// rule, because at the launch there is nothing to have put back yet. Reaching it for a
    /// plane this process is already holding would start a second copy of every chat the
    /// record names, beside the copies already running — so the question is asked here, where
    /// both callers pass, rather than at each of them.
    fn minted(&self, root: &Path, record: Read) -> PlaneId {
        let already = self.held(&PlaneId::of(root)).is_ok();
        let id = self.open(root);
        if !already {
            self.reopen(&id, &Approved(()), record);
        }
        id
    }

    /// What this machine's store says about opening `root` as it stands right now.
    ///
    /// **Every unreadable state is "ask".** No config home, a store charter would not read, a
    /// platform charter keeps no store on: [`machine::read`] answers with an empty store in
    /// each case, and an empty store's [`machine::Store::consent`] is
    /// [`machine::Consent::New`]. Silence is never a yes — `profiletrust` opens its module
    /// documentation refusing exactly that, and this is the same answer about a bigger grant.
    fn consent_to(&self, root: &Path, contributes: &machine::Contribution) -> machine::Consent {
        let Some(config) = self.config.as_deref() else {
            return machine::Consent::New;
        };
        machine::read(config).store.consent(root, contributes)
    }

    /// Puts `root` at the front of this machine's list of planes.
    ///
    /// **Never an approval.** `Store::remember` carries an existing [`machine::Trust`] over
    /// and creates none, so opening a plane a hundred times does not become consent to it.
    fn remember(&self, root: &Path) {
        let Some(config) = self.config.as_deref() else {
            return;
        };
        let when = now();
        let plane = root.to_path_buf();
        if let Err(why) = machine::update(config, move |store| store.remember(&plane, when))
            && why.kind() != std::io::ErrorKind::Unsupported
        {
            eprintln!(
                "charter: {} was opened but not added to the list of recent planes ({why})",
                root.display()
            );
        }
    }

    /// Writes the operator's answer down, so they are asked once per plane per machine.
    ///
    /// Best effort, and never worth refusing the open over: an approval that could not be
    /// filed costs one more question at the next launch, and refusing here would cost the
    /// operator the project they just said yes to. `Unsupported` is silent because it is not
    /// a fault — it is a platform with no store, and the opener says so in its own words
    /// rather than on a standard error nobody double-clicking an icon can read.
    fn record_approval(&self, root: &Path, contributed: machine::Contribution) {
        let Some(config) = self.config.as_deref() else {
            return;
        };
        let when = now();
        let plane = root.to_path_buf();
        if let Err(why) = machine::update(config, move |store| {
            store.approve(&plane, when, contributed);
        }) && why.kind() != std::io::ErrorKind::Unsupported
        {
            eprintln!(
                "charter: {} was opened, but your approval of it was not recorded ({why}); \
                 charter will ask about it again",
                root.display()
            );
        }
    }

    /// Writes down which projects each window is holding, so the next cold launch can put
    /// them back (ADR 0033, spec decision 28).
    ///
    /// **Written whenever the arrangement changes, not only at the quit.** The record is the
    /// same either way at a quit, and a charter that was killed — or a machine that lost
    /// power — still comes back to the projects that were open. `RunEvent::Exit` was the
    /// other candidate and it is the one that loses everything to a crash.
    ///
    /// **Ids are turned back into roots HERE**, through the registry, rather than by spelling
    /// a `PlaneId` back into a path. The id is a `display()` of the root, which SUBSTITUTES
    /// for a byte that is not UTF-8 — so a plane whose path is not UTF-8 would be written down
    /// as a path that is not the one that is open, and then opened at the next launch.
    ///
    /// Never worth refusing anything over: an arrangement that could not be filed costs the
    /// next launch its tabs, and there is nothing the operator could do about it here.
    /// `Unsupported` is silent, because it is Windows having no store at all (ADR 0031) rather
    /// than a fault.
    pub fn remember_arrangement(&self, windows: &[Holding]) {
        let Some(config) = self.config.as_deref() else {
            return;
        };
        let arranged: Vec<machine::Window> = {
            let open = self.map();
            windows
                .iter()
                .filter_map(|holding| {
                    let front = holding.front();
                    let kept: Vec<(&PlaneId, PathBuf)> = holding
                        .planes
                        .iter()
                        .filter_map(|id| Some((id, open.get(id)?.root.clone())))
                        .collect();
                    if kept.is_empty() {
                        return None;
                    }
                    // The index is found again in the list that SURVIVED, never carried over
                    // from the one that went in: a plane the registry has already let go of
                    // shifts everything after it, and an `active` off by one is a window that
                    // comes back on the wrong project.
                    let active = front
                        .and_then(|front| kept.iter().position(|(id, _)| *id == front))
                        .unwrap_or(0);
                    Some(machine::Window {
                        planes: kept.into_iter().map(|(_, root)| root).collect(),
                        active,
                    })
                })
                .collect()
        };
        if let Err(why) = machine::update(config, move |store| store.windows = arranged)
            && why.kind() != std::io::ErrorKind::Unsupported
        {
            eprintln!(
                "charter: the projects this window holds were not written down ({why}); the \
                 next launch will not put them back"
            );
        }
    }

    /// Pins or unpins a project, or one of its workspaces, in the machine store.
    ///
    /// **This one refuses rather than shrugging**, unlike `remember` and `record_approval`
    /// above, and the difference is who is waiting. Those two are charter's own bookkeeping
    /// on a path the operator is already walking; this is a button they just pressed, and a
    /// pin that silently did not happen is a control that does not work. So the store's own
    /// sentence travels back — including the one a machine with no store gives
    /// (`Unsupported`, which on Windows is ADR 0031's refusal and is the honest answer to
    /// "pin this": charter cannot, here, and says so).
    pub fn pin(&self, root: &Path, workspace: Option<&str>, pinned: bool) -> Result<(), String> {
        let Some(config) = self.config.as_deref() else {
            return Err(
                "charter has no config home on this machine, so it cannot remember a pin."
                    .to_owned(),
            );
        };
        let plane = root.to_path_buf();
        let workspace = workspace.map(str::to_owned);
        // The store's own refusal, out of the closure: `update` answers an `io::Error`, and
        // wrapping a bound the operator can act on ("unpin one first") in one would turn a
        // sentence they can follow into a sentence about a file.
        let mut refused = None;
        machine::update(config, |store| {
            refused = match &workspace {
                Some(name) => store.pin_workspace(&plane, name, pinned).err(),
                None => store.pin(&plane, pinned).err(),
            };
        })
        .map_err(|why| format!("charter could not write the pin down: {why}"))?;
        match refused {
            Some(why) => Err(why),
            None => Ok(()),
        }
    }

    /// Everything this machine remembers, with what it would not take back named.
    ///
    /// The store alone: whether each remembered path is still THERE is a `stat` per row and
    /// is deliberately not asked here — see [`machine::still_a_plane`], and the caller that
    /// asks it off the thread that draws.
    pub fn remembered(&self) -> machine::Loaded {
        match self.config.as_deref() {
            Some(config) => machine::read(config),
            // No config home at all. Not an error and not a refusal: it is a machine that
            // remembers nothing, which is the same shape as a first launch.
            None => machine::Loaded::default(),
        }
    }

    /// Everything one plane needs, built and wired but not yet started.
    fn hold(&self, id: PlaneId, root: PathBuf) -> Held {
        // A socket that cannot be opened is not worth refusing to open a plane over: it comes
        // up with every chat `unknown` and says so, which is a working project with one
        // feature missing rather than no project at all.
        let at = hooks::socket_for(Some(&root));
        let hooks =
            Hooks::listening_on(id.clone(), &at, Arc::clone(&self.tell)).unwrap_or_else(|why| {
                eprintln!(
                    "charter: no hook channel at {} ({why}); every chat in {} will show as \
                     unknown",
                    at.socket.display(),
                    root.display()
                );
                Hooks::deaf(id.clone())
            });
        let reporting = hooks.socket().map(|socket| Reporting {
            socket: socket.to_path_buf(),
        });

        let records = Arc::new(Records {
            root: root.clone(),
            config: self.config.clone(),
            allowed: AtomicBool::new(false),
        });
        let writes = Arc::clone(&records);
        let mut chats = Chats::recorded_by_reporting_to(
            Box::new(move |record| writes.write(record)),
            reporting,
        );
        chats.arming_with(self.shipped.clone());

        // **Before a single session is started, because putting the record back starts them.**
        // A harness fires `SessionStart` at its own exec, and a board that learned the chat's
        // number afterwards would miss it — for a chat that is then idle, waiting for a first
        // prompt, no second event ever comes and it reads `unknown` for the rest of the run.
        {
            let board = hooks.shared_board();
            chats.when_one_starts(Box::new({
                let board = Arc::clone(&board);
                move |session, harness, conversation| {
                    hooks::held_board(&board).opened(session, harness, conversation);
                }
            }));
            // And taken back if the program then fails to start: the announcement has to come
            // first, so it can be about a chat that never happens.
            chats.when_one_does_not_start(Box::new(move |session| {
                hooks::held_board(&board).closed(session);
            }));
        }
        // No hook can report a program dying (the process is gone), so the operating system
        // does. That is not charter reading a harness's output (ADR 0018) — it is the
        // process's own exit status, and the only honest source for `failed`.
        {
            let board = hooks.shared_board();
            let tell = Arc::clone(&self.tell);
            let plane = id.clone();
            chats
                .sessions()
                .when_one_ends(Box::new(move |session, exit| {
                    let changed = hooks::held_board(&board).exited(session, hooks::code_of(&exit));
                    if changed {
                        tell(hooks::now(&board, plane.clone(), session));
                    }
                }));
        }

        // Never fatal, as the socket above is not: a plane that cannot be watched still opens,
        // and its panels read it when a workspace is focused.
        let watch = crate::planewatch::Watch::start(id.clone(), &root, Arc::clone(&self.changes))
            .map_err(|why| {
                eprintln!(
                    "charter: {} is not watched ({why}); its panels will not follow changes \
                     made outside this window",
                    root.display()
                );
            })
            .ok();

        Held {
            id,
            root,
            hooks,
            tell: Arc::clone(&self.tell),
            chats,
            records,
            watch: Mutex::new(watch),
        }
    }

    /// Closes a plane: its record is written, its sessions are ended, its socket is released.
    ///
    /// **Nothing of the plane on disk is touched** beyond the record the app already keeps
    /// there. Closing a project is the app letting go of it, never the plane going away.
    pub fn close(&self, plane: &PlaneId) -> Result<(), String> {
        let held = self
            .map()
            .remove(plane)
            .ok_or_else(|| no_such(plane, "close"))?;
        held.let_go();
        Ok(())
    }

    /// The plane a command is acting for, or the one sentence saying it is not open.
    pub fn held(&self, plane: &PlaneId) -> Result<Arc<Held>, String> {
        self.map()
            .get(plane)
            .map(Arc::clone)
            .ok_or_else(|| no_such(plane, "act on"))
    }

    /// Every plane this process holds, in no particular order.
    pub fn open_now(&self) -> Vec<PlaneId> {
        let mut open: Vec<_> = self.map().keys().cloned().collect();
        open.sort_by(|one, two| one.0.cmp(&two.0));
        open
    }

    /// The way out: every plane's record written, every plane's sessions ended.
    ///
    /// Tauri ends the process itself, which runs no destructor and waits for no thread, so
    /// this is called from the exit event and not left to a drop that never happens.
    ///
    /// The registry is emptied FIRST and let go of afterwards. Ending a session tells the
    /// board, which tells the window, which asks this registry what the chat is called — and
    /// a lock still held here would be a deadlock on the way out, in the one path an operator
    /// cannot escape by clicking something else.
    pub fn let_go_of_all(&self) {
        let all: Vec<_> = self.map().drain().map(|(_, held)| held).collect();
        for held in all {
            held.let_go();
        }
    }

    /// The registry, whether or not a thread panicked while holding it. What it holds is
    /// still the best answer there is, and refusing to draw anything at all would be worse.
    fn map(&self) -> MutexGuard<'_, HashMap<PlaneId, Arc<Held>>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The one sentence for a plane the process is not holding.
///
/// It names the plane, because with several open "no plane" alone tells the operator nothing
/// about which one went.
fn no_such(plane: &PlaneId, doing: &str) -> String {
    format!(
        "charter has no plane open at {}, so there is nothing to {doing}",
        charter_core::shown::short(plane.as_str())
    )
}

/// What came of being asked to open a plane.
pub enum Opening {
    /// It is open, and this is the id every later call names it by.
    Open(PlaneId),
    /// The operator has to be asked first. **Nothing was attached and nothing was started.**
    Ask(Asking),
}

/// What the operator is being asked about, when a plane must be approved before it opens.
pub struct Asking {
    /// The plane's root as charter resolved it — canonical, and the spelling the approval is
    /// recorded against. Never the one the caller typed: an approval filed under one spelling
    /// of a directory and read back under another is an approval nobody gets to use.
    pub root: PathBuf,
    /// What it would contribute, as it is on this disk at this instant.
    pub contributes: machine::Contribution,
    /// Whether nothing has approved it, or it now does more than what was approved — and, for
    /// the second, every way it differs.
    pub consent: machine::Consent,
}

impl Asking {
    /// Whether nothing has approved this plane at all, rather than its having changed since it
    /// was approved.
    ///
    /// The two read differently to an operator — one is "open this?" and the other is "this is
    /// not what you said yes to" — so the difference is named once, here, and not spelled out
    /// again by every caller that has to tell them apart.
    pub fn first(&self) -> bool {
        matches!(self.consent, machine::Consent::New)
    }
}

/// The plane a path names, resolved to the one spelling everything downstream uses.
///
/// **`find_root`, never `resolve`.** `resolve` honours `$CHARTER_ROOT`, so a window built on
/// it would answer "the plane you picked" with a completely different directory whenever that
/// variable happened to be set — an operator picking a folder and being handed somebody else's
/// plane. `find_root` walks up from the directory itself and looks at nothing else, which is
/// what "open this folder" means. (The launch's own resolution is the opposite case and stays
/// on `resolve`: there the variable IS the operator saying which plane they meant.)
///
/// **Canonicalised first, and a symlink is followed rather than refused** — which is the
/// other way round from [`machine::still_a_plane`], deliberately. A path the operator is
/// picking right now has no approval behind it yet, so following the link and approving what
/// it really points at is honest. A REMEMBERED path that has become a link is the case where
/// an approval already exists for whatever it pointed at before, and charter cannot tell the
/// two apart, so that one is dropped.
fn plane_at(root: &Path) -> Result<PathBuf, String> {
    let shown = charter_core::shown::short(&root.display().to_string());
    let here = root
        .canonicalize()
        .map_err(|why| format!("charter cannot open {shown}: {why}"))?;
    charter_core::plane::find_root(&here).map_err(|_| {
        format!(
            "{shown} is not a plane: charter found no {} there or in any directory above it",
            charter_core::plane::MANIFEST
        )
    })
}

/// Now, in seconds since the epoch — and zero on a clock that says it is before 1970, because
/// a timestamp in the recents list is worth no launch at all.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// What one window is holding: its projects as tabs, and which of them is in front.
///
/// **The tabs and the front one are one value, not two.** A window that told charter its front
/// plane in one call and its tab strip in another would have two answers to "what is on
/// screen" and nothing keeping them in step — which is the singleton `Plane` ADR 0033 had to
/// undo, wearing a tab bar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Holding {
    /// Left to right, as the project tabs show them.
    pub planes: Vec<PlaneId>,
    /// Which tab is in front. `None` is the opener: the window holds projects the operator is
    /// not looking at, or holds none at all.
    pub active: Option<usize>,
}

impl Holding {
    /// The plane the operator is actually looking at, if any.
    ///
    /// An `active` past the end reads as none rather than panicking or wrapping: it can only
    /// arrive from a window that is mid-change, and "not looking" is the answer that sends a
    /// notification rather than swallowing one.
    pub fn front(&self) -> Option<&PlaneId> {
        self.planes.get(self.active?)
    }
}

/// What each window is holding, and which of its projects it has in front.
///
/// **This is the map #111 named as missing, and the reason it was missing is the reason it is
/// needed.** `already_looking_at` asks a chat's OWN plane whether that chat is in front,
/// because every plane numbers its chats from one — and with two planes open that is only
/// half the question. Plane A's chat 3 can be in front *of plane A* while the window is
/// showing plane B, and the notification the operator actually needed is the one that gets
/// suppressed. The window is the only thing that knows which plane it is drawing, so the
/// window says.
///
/// It is also what a cold launch restores from: the arrangement written into this machine's
/// store is [`Self::arrangement`] and nothing else, so "what charter remembers about the
/// window" and "what the window told charter" cannot disagree.
///
/// Keyed by window label rather than held as one value, because a second window is the shape
/// this app is going to (ADR 0033) and a singleton here would be the singleton `Plane` that
/// ADR 0033 had to undo, one layer up.
#[derive(Default)]
pub struct Showing(Mutex<HashMap<String, Holding>>);

impl Showing {
    /// A window says what it is holding and which project it has in front.
    ///
    /// A window holding nothing is forgotten rather than recorded as empty: it is an opener
    /// with nothing open, and an empty window in the arrangement would restore as nothing at
    /// the next launch while still being a row charter had to write down.
    pub fn in_window(&self, window: &str, holding: Holding) {
        let mut showing = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if holding.planes.is_empty() {
            showing.remove(window);
        } else {
            showing.insert(window.to_owned(), holding);
        }
    }

    /// Whether `window` has `plane` in front.
    ///
    /// A window that has never said is treated as showing nothing, so a notification is sent
    /// rather than suppressed. That is the cheap way round: a notification the operator did
    /// not need costs a glance, and one they needed and did not get costs a chat sitting
    /// unanswered.
    ///
    /// **A project the window HOLDS but is not looking at is not in front**, which is the
    /// whole of what project tabs added to this question: fifty chats can be live in a tab
    /// behind the one on screen, and a notification about one of them is exactly the
    /// notification the operator needs.
    pub fn is_showing(&self, window: &str, plane: &PlaneId) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(window)
            .and_then(Holding::front)
            == Some(plane)
    }

    /// Every window's arrangement, in a stable order.
    ///
    /// Ordered by window label rather than by whatever the map iterates, so that writing the
    /// arrangement twice with nothing changed writes the same bytes twice.
    pub fn arrangement(&self) -> Vec<Holding> {
        let showing = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut labels: Vec<&String> = showing.keys().collect();
        labels.sort();
        labels
            .into_iter()
            .filter_map(|label| showing.get(label).cloned())
            .collect()
    }
}

/// Whether this launch puts back the window set charter remembers.
///
/// `--no-restore` starts clean (spec decision 28), and it exists for the launch after the one
/// that restored something the operator did not want. It is read from the process's own
/// arguments once, at startup, and never again: a second launch's arguments reach the running
/// process through the single-instance plugin, and a `--no-restore` typed then would be about
/// a restore that happened hours ago.
pub struct Restoring(bool);

/// The argument that starts clean. Spelled once, so the flag and the test for it are one word.
pub const NO_RESTORE: &str = "--no-restore";

impl Restoring {
    /// What this launch's arguments say.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        Self(!args.into_iter().any(|arg| arg == NO_RESTORE))
    }

    pub fn wanted(&self) -> bool {
        self.0
    }
}

/// The window set a cold launch has to put back, and every project it would not take back.
pub struct Restorable {
    /// The projects to open again, left to right as the tabs were.
    pub planes: Vec<PathBuf>,
    /// Which of them was in front, as an index into `planes` **after** the drops — so a
    /// window whose front project has gone comes back on one that is still there.
    pub active: Option<usize>,
    /// One line per project charter would not take back.
    pub dropped: Vec<String>,
}

/// The arrangement charter remembered, checked against this disk.
///
/// **A project that has moved or is gone is dropped with a line saying so, never an error
/// dialog** (ADR 0033). It is `put_back`'s rule for a chat whose profile has gone, and it is
/// the same reason: a restore is a convenience, and a convenience that blocks the launch is
/// worse than the thing it was restoring.
///
/// **Nothing here opens anything.** Opening a project starts the programs its record names, so
/// it goes through the trust gate like every other open — this only says which projects to
/// offer that gate. A restore that minted its own approval would be ADR 0035 turned off for
/// every project the operator had ever had open at once.
///
/// **Every remembered window merges into one.** charter draws one OS window today; splitting a
/// tab back out needs multi-window Tauri and is not here yet. Merging is the answer that keeps
/// the operator's projects — dropping the second window's tabs would lose work to a limitation
/// they never asked for.
pub fn restorable(loaded: machine::Loaded) -> Restorable {
    let mut planes: Vec<PathBuf> = Vec::new();
    let mut active = None;
    // What the store itself would not take back, already dropped with a reason by the read.
    // Only the window rows: a remembered RECENT charter would not take back is the opener's
    // news, and saying it twice on one launch is saying it twice.
    let mut dropped: Vec<String> = loaded
        .dropped
        .iter()
        .filter(|why| matches!(why, machine::Dropped::Window { .. }))
        .map(ToString::to_string)
        .collect();
    for (which, window) in loaded.store.windows.into_iter().enumerate() {
        let was_active = window.active;
        for (at, plane) in window.planes.into_iter().enumerate() {
            let shown = charter_core::shown::short(&plane.display().to_string());
            if let Err(why) = machine::still_a_plane(&plane) {
                dropped.push(format!("{shown} {why}"));
                continue;
            }
            // One project is one tab. Two windows that both held it merge into one that
            // holds it once, and a second tab on one plane would be a second `PlaneView`
            // drawing one board.
            if planes.contains(&plane) {
                continue;
            }
            if which == 0 && at == was_active {
                active = Some(planes.len());
            }
            planes.push(plane);
        }
    }
    // A front tab that was dropped leaves the window on the first project that survived,
    // rather than on none: the operator asked for these projects, and an opener in front of
    // them is a screen they have to click past.
    if active.is_none() && !planes.is_empty() {
        active = Some(0);
    }
    Restorable {
        planes,
        active,
        dropped,
    }
}

/// The size a session starts at. The pane it lands in tells it the real one at once, and a
/// chat put back at a launch has no pane yet to ask.
const STARTING: Size = Size {
    columns: 80,
    rows: 24,
};

/// What a launch had to go on, and what came of it.
///
/// **Three states, and none of them is an error.** The app has to come up holding no plane at
/// all and stay useful — that is what an opener attaches to — and "there is no plane where you
/// launched me" is a different thing to tell an operator from "you have not opened one yet".
/// A window double-clicked from the dock has a working directory of `/` and is the second;
/// `charter` run in a directory that is in no plane is the first.
///
/// They are told apart by shape rather than by reading a sentence: `plane` set is a plane in
/// hand, `from` set without it is a directory that is in no plane, and neither is a launch
/// that was given nothing to go on.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct Launch {
    /// The plane this launch opened, and the id every command for it names.
    pub plane: Option<PlaneId>,
    /// The directory the launch was given, when it could be read. A HINT and nothing more:
    /// it is resolved once, here, and no command consults the working directory again.
    pub from: Option<String>,
    /// Why no plane was opened, in the resolver's own words.
    pub why: Option<String>,
}

/// Resolves the launch's working directory to a plane, and opens it.
///
/// **One rule, one answer, and this line is the whole of it.** `charter_core::plane::resolve`
/// is the resolver the CLI asks and the one every command in this app used to ask — except
/// the launch, which asked `find_root`. The two disagree about `$CHARTER_ROOT`, so under the
/// scenario tests the app bound its hook socket and wrote its record against one plane while
/// the sidebar, the picker and every launch read another. Two resolvers disagreeing in a
/// worktree is what M2.16 cost a day to find; this was the same defect one layer up.
///
/// After this, the working directory is never consulted again: a plane is named explicitly
/// from here on.
pub fn at_launch(planes: &Planes, cwd: std::io::Result<PathBuf>) -> Launch {
    resolving_with(planes, cwd, |cwd| {
        charter_core::plane::resolve(cwd).map_err(|why| why.to_string())
    })
}

/// [`at_launch`], with the resolver named — so a test can drive the three states without
/// reaching into the process's environment, which it shares with every other test.
fn resolving_with(
    planes: &Planes,
    cwd: std::io::Result<PathBuf>,
    resolve: impl FnOnce(&Path) -> Result<PathBuf, String>,
) -> Launch {
    let cwd = match cwd {
        Ok(cwd) => cwd,
        // Nothing to go on at all. Not an error: an app launched from an icon has no useful
        // working directory to speak of, and it still has to come up.
        Err(err) => {
            return Launch {
                plane: None,
                from: None,
                why: Some(format!("charter cannot read the current directory: {err}")),
            };
        }
    };
    let from = Some(cwd.display().to_string());
    match resolve(&cwd) {
        Ok(root) => {
            let plane = planes.open(&root);
            // **The first of the two approvals this module mints.** The operator ran charter
            // in this directory; that act is the yes, and it is the yes for THIS plane and no
            // other.
            //
            // `open` above remembered it but did NOT approve it, and the difference is ADR
            // 0035's: running charter here is consent to open this plane now, and the
            // recorded approval is an answer to a question the operator was shown and read.
            // So this plane appears in the opener's list — an operator who has only ever
            // launched from a terminal must not find that list empty — and opening it from
            // that list asks once, as every other plane does.
            //
            // **It is not put back HERE** (charter-app#250). The launch asks the operator
            // whether they want what was open back, and nothing starts before the answer — so
            // the plane is written down as owed, and [`Planes::relaunch`] spends this yes when
            // the window has the answer. That is where the record is read, once, against the
            // root the registry settled on (`record_of`): `Planes::open` canonicalises, and
            // reading `/var/…` while the plane is held as `/private/var/…` is the
            // two-spellings-of-one-directory defect the id exists to stop.
            planes.relaunching().owed = Some(plane.clone());
            Launch {
                plane: Some(plane),
                from,
                why: None,
            }
        }
        Err(why) => {
            eprintln!(
                "charter: no plane here, so nothing is reopened and nothing is recorded \
                 (a plane is the nearest directory at or above this one with a charter.toml)"
            );
            Launch {
                plane: None,
                from,
                why: Some(why),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planes() -> Planes {
        Planes::telling(Arc::new(|_: Moved| {}), crate::Shipped::default(), None)
    }

    /// A plane on disk, with nothing in it but the marker that makes it one.
    fn a_plane(at: &Path) -> PathBuf {
        std::fs::create_dir_all(at).expect("the plane's directory");
        std::fs::write(at.join(charter_core::plane::MANIFEST), "").expect("its charter.toml");
        at.to_path_buf()
    }

    /// A Rust file's CODE, with every comment taken out.
    ///
    /// **The audit below is about what the crate does, and a rule is worth writing down.**
    /// The first spelling read the text whole and so failed on its own explanation — on the
    /// comment over `worktree_of_chat` naming the walk it no longer makes, on the paragraph
    /// in this very test — which would have taught the next person to document the rule less.
    ///
    /// It cuts at the first `//` on a line and does not care that a `//` inside a string
    /// literal is one too: a needle hidden behind one is the only thing that escapes, and
    /// spelling `plane::resolve` inside a string to get past an audit is not a mistake
    /// anybody makes by accident. Being a Rust parser here would cost more than the check.
    fn code_of(text: &str) -> String {
        text.lines()
            .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every `.rs` file of this crate, as `(name, code with the comments taken out)`.
    ///
    /// Anchored to `CARGO_MANIFEST_DIR` and never to the working directory, for the reason
    /// `BINDINGS` is: a test is run from wherever the runner stands.
    fn the_app_crates_sources() -> Vec<(String, String)> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found: Vec<(String, String)> = std::fs::read_dir(&src)
            .expect("the app crate's own sources are readable")
            .filter_map(Result::ok)
            .map(|item| item.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
            .map(|path| {
                let name = path
                    .file_name()
                    .expect("a file has a name")
                    .to_string_lossy()
                    .into_owned();
                let text = std::fs::read_to_string(&path).expect("it is readable");
                (name, code_of(&text))
            })
            .collect();
        found.sort();
        assert!(
            found.len() > 10,
            "the audit below read {} files, which is not this crate",
            found.len()
        );
        found
    }

    /// **The audit charter-app#127 asked to keep, run rather than remembered.**
    ///
    /// #111 made the registry the only route to a board so that *a command that forgets which
    /// plane it means does not compile*, and the type carries that as far as a type can: a
    /// [`PlaneId`] is minted by [`Planes::open`] alone and [`Planes::held`] refuses one this
    /// process never opened. What the type cannot do is stop a command from **not asking** —
    /// from taking a path of its own and walking up from it, which is what `worktree_list`,
    /// `worktree_remove` and `worktree_merge` did with a `String`, what `worktree_of_chat` did
    /// with a chat's `cwd`, and what `workspace_panels` and `workspace_repos` did with
    /// `current_dir()` before #125.
    ///
    /// So the audit the issue names — "grep `current_dir` and any command taking a plane as
    /// `String`" — is written down here as the thing it is: **the walk up from a path to a
    /// plane belongs to this module, and the process's own directory belongs to `setup`.** A
    /// fourth command that reached for either has to edit this list to land, and editing it is
    /// a decision somebody makes on purpose rather than an argument they forgot to pass.
    #[test]
    fn nothing_but_this_module_turns_a_path_into_the_plane_a_command_acts_on() {
        // Spelled in pieces because this file is read by the audit like any other, and a
        // needle written whole here would be a use of the very thing being looked for.
        let walking_up = ["plane", "::resolve"].concat();
        let the_processs_own_directory = ["current", "_dir"].concat();
        let mut wrong = Vec::new();
        let (mut resolves_here, mut launched_in) = (0, 0);
        for (name, code) in the_app_crates_sources() {
            // `planes.rs` is where a path becomes a plane: `Planes::open` resolves the root
            // the operator named and mints the id every command is then handed.
            let walks = code.matches(&walking_up).count();
            if name == "planes.rs" {
                resolves_here += walks;
            } else if walks > 0 {
                wrong.push(format!("{name} walks up from a path to a plane"));
            }
            // `lib.rs`, once, in `setup`: the directory charter was launched in is one of the
            // operator's two yeses (`Approved`), and it is read there and nowhere else.
            let directories = code.matches(&the_processs_own_directory).count();
            if name == "lib.rs" {
                launched_in += directories;
            } else if directories > 0 {
                wrong.push(format!(
                    "{name} reads the process's own directory {directories} time(s)"
                ));
            }
        }
        assert!(
            wrong.is_empty(),
            "charter-app#127: a command is choosing its own plane rather than being handed \
             one the registry vouched for — {wrong:?}"
        );
        // ...and the two the rule PERMITS are really there, so a `code_of` that started
        // returning nothing would fail here rather than report the crate spotless.
        assert!(
            resolves_here > 0,
            "the registry no longer resolves a plane at all, so the check above read nothing"
        );
        assert_eq!(
            launched_in, 1,
            "the process's own directory is read in `setup` and nowhere else (#125)"
        );
    }

    /// The same audit at the window's edge: **every command that names a plane names it as a
    /// [`PlaneId`]**, which is the half a reader of the TypeScript can check.
    ///
    /// The bindings are generated from the command list and CI holds them to being in sync
    /// (`the_typescript_the_ui_imports_is_the_one_these_commands_generate`), so a claim about
    /// that file is a claim about the commands. Cheap, and it is the shape the three commands
    /// in #127's title were wrong in: `plane: string`.
    #[test]
    fn no_command_takes_its_plane_as_a_bare_string() {
        let bindings = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts"),
        )
        .expect("the generated bindings are readable");
        let raw: Vec<&str> = bindings
            .lines()
            .filter(|line| line.contains("__TAURI_INVOKE("))
            .filter(|line| {
                line.contains("plane: string")
                    || line.contains("planeRoot: string")
                    || line.contains("root: string")
            })
            .collect();
        assert!(
            raw.is_empty(),
            "charter-app#127: a command takes a plane PATH, which is whatever the caller says, \
             where a `PlaneId` is one the registry agreed to — {raw:?}"
        );
        // ...and the mirror, so this cannot go vacuous the day the bindings stop being
        // generated at all: there ARE commands here, and they do name planes.
        assert!(
            bindings.matches("plane: PlaneId").count() > 10,
            "the bindings name no planes at all, so the check above proved nothing"
        );
    }

    /// Where a held plane is listening, as a path a test can look for on disk.
    fn socket_of(planes: &Planes, plane: &PlaneId) -> Option<PathBuf> {
        planes
            .held(plane)
            .expect("the plane is held")
            .hooks()
            .socket()
            .map(Path::to_path_buf)
    }

    #[test]
    fn a_command_can_only_reach_a_plane_the_process_is_holding() {
        // The whole point of the registry: there is no ambient board to reach for, so a
        // plane that is not open answers with a sentence naming it rather than with
        // somebody else's chats.
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();

        let refused = planes
            .held(&PlaneId::of(&a_plane(&dir.path().join("plane"))))
            // `Held` holds a listener thread and a table of terminals, so it is not `Debug`.
            // The refusal is what this is about; the plane itself is dropped to read it.
            .map(|_| ())
            .expect_err("a plane nobody opened is not held");

        assert!(refused.contains("no plane open at"), "{refused}");
    }

    #[test]
    fn opening_the_same_plane_twice_holds_it_once_and_never_rebinds_its_socket() {
        // `Listener::bind` REMOVES a socket left behind by a process that is gone, which is
        // right for a stale one and a disaster for a live one: every session of the plane
        // already open carries that path in its environment, so their hooks would report
        // onto a second board that numbers its chats from one, and every state would land on
        // the wrong chat.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes();

        let first = planes.open(&root);
        let held = planes.held(&first).expect("it is held");
        let again = planes.open(&root);

        assert_eq!(first, again);
        assert_eq!(planes.open_now(), vec![first.clone()]);
        // The SAME holding, not an equal one: a second `Held` is a second board, a second
        // `Chats` numbering from one, and a second `bind` over a live socket.
        assert!(
            Arc::ptr_eq(&held, &planes.held(&again).expect("it is held")),
            "the second open built a second holding of one plane"
        );
        assert!(
            socket_of(&planes, &again).is_some_and(|socket| socket.exists()),
            "the plane stopped listening"
        );
    }

    #[test]
    fn opening_a_plane_does_not_run_what_its_record_names() {
        // `.charter/app/reopen.json` is an execution input: putting it back starts the
        // programs it names, and for a chat that was not on a profile what runs is decided
        // from the record alone. A plane is a directory, and a directory arrives by zip or
        // on a stick. Attaching one must not be running one.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let wrote = a_record_naming(&root, "/bin/echo");
        let planes = planes();

        let plane = planes.open(&root);

        let held = planes.held(&plane).expect("it is held");
        assert!(
            held.chats().open_now().is_empty(),
            "a plane nobody approved started a program out of its record"
        );
        assert!(held.chats().would_not_start().is_empty(), "it even tried");
        assert_eq!(
            std::fs::read(record_of(&root)).expect("the record is still there"),
            wrote,
            "a plane nobody approved had its record written over"
        );
    }

    #[test]
    fn closing_a_plane_nobody_approved_leaves_its_record_exactly_as_it_was() {
        // The other half: an attached plane holds no chats, so writing its record on the way
        // out would replace the operator's own list — of a plane they never said yes to —
        // with an empty one.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let wrote = a_record_naming(&root, "/bin/echo");
        let planes = planes();
        let plane = planes.open(&root);

        planes.close(&plane).expect("it closes");

        assert_eq!(
            std::fs::read(record_of(&root)).expect("the record is still there"),
            wrote
        );
    }

    /// A record of one chat running `program` and nothing else.
    fn one_chat_on(program: &str) -> reopen::Record {
        reopen::Record {
            views: Vec::new(),
            chats: vec![charter_core::reopen::Chat {
                program: program.to_owned(),
                args: Vec::new(),
                cwd: None,
                name: "one".to_owned(),
                resume: None,
                active: true,
                profile: None,
                persona: None,
                show_footer: false,
                pinned: false,
                number: None,
            }],
            dealt: 0,
            relaunch_after_update: false,
        }
    }

    /// A record in `root` naming one chat on `program`, and the bytes it left on disk.
    fn a_record_naming(root: &Path, program: &str) -> Vec<u8> {
        reopen::write(root, &one_chat_on(program)).expect("the record is written");
        std::fs::read(record_of(root)).expect("the record reads back")
    }

    /// A record in `root` naming one chat per program, in order.
    #[cfg(unix)]
    fn a_record_naming_each(root: &Path, programs: &[&str]) {
        let chats = programs
            .iter()
            .enumerate()
            .map(|(which, program)| charter_core::reopen::Chat {
                program: (*program).to_owned(),
                args: Vec::new(),
                cwd: None,
                name: format!("chat.{which}"),
                resume: None,
                active: which == 0,
                profile: None,
                persona: None,
                show_footer: false,
                pinned: false,
                number: None,
            })
            .collect();
        reopen::write(
            root,
            &reopen::Record {
                chats,
                ..Default::default()
            },
        )
        .expect("the record is written");
    }

    #[cfg(unix)]
    #[test]
    fn a_record_written_after_the_operator_clicked_is_not_the_one_that_is_started() {
        // charter-app#123, at the only place it can be shown: a write that lands **between**
        // the comparison `approve_and_open` makes and the record `put_back` executes.
        //
        // The comparison itself was already there — it is why a plane that changed while the
        // dialog was open refuses — so a test that writes BEFORE the call is testing that, and
        // a test that writes AFTER it is testing nothing. The seam puts the writer in the
        // window, which is the whole defect.
        //
        // Counted rather than looked at, as every other test of `put_back` here is: a program
        // that dies at once is a chat that was TRIED, and on a runner either answer is honest.
        // What must not happen is that three chats were tried when the operator read one.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        a_record_naming(&root, "/bin/echo");
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;
        assert_eq!(shown.starts.len(), 1, "the dialog drew one chat");

        let planting = root.clone();
        let plane = planes
            .approving(&root, &shown, || {
                // A chat running in another plane, a shell, anything with write access to this
                // directory — the operator has clicked and is not looking at it any more.
                a_record_naming_each(&planting, &["/bin/echo", "/bin/echo", "/bin/echo"]);
            })
            .expect("the operator said yes to what they were shown");

        let held = planes.held(&plane).expect("it is held");
        assert_eq!(
            held.chats().open_now().len() + held.chats().would_not_start().len(),
            1,
            "the record that was executed is not the record that was approved: a write that \
             landed after the click was started without ever having been drawn"
        );
    }

    /// Whether the store would stop to ask about this plane, as it stands right now.
    #[cfg(unix)]
    fn would_ask(config: &Path, root: &Path) -> bool {
        machine::read(config)
            .store
            .consent(root, &machine::Contribution::of(root))
            .must_ask()
    }

    #[cfg(unix)]
    #[test]
    fn writing_a_plane_s_record_vouches_for_it_in_the_same_breath() {
        // charter rewrites `.charter/app/reopen.json` every time a chat opens or closes, and
        // the store's fingerprint covers the programs that record would start. A fingerprint
        // refreshed only when the operator approved the plane goes stale on their very next
        // click — and the next launch asks them about a chat they started themselves. An
        // operator trained to dismiss that question is worse off than one never asked.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        machine::update(&config, |store| {
            store.remember(&root, 1);
            store.approve(&root, 1, machine::Contribution::of(&root));
        })
        .expect("the plane is approved");
        let records = Records {
            root: root.clone(),
            config: Some(config.clone()),
            allowed: AtomicBool::new(true),
        };

        // A record that appeared behind charter's back is exactly what the question is for.
        reopen::write(&root, &one_chat_on("/bin/echo")).expect("the record is written");
        let behind_its_back = would_ask(&config, &root);
        // And one charter itself wrote is not.
        records.write(&one_chat_on("/bin/true"));
        let charter_s_own = would_ask(&config, &root);

        assert!(
            behind_its_back,
            "a record charter did not write must still be asked about"
        );
        assert!(
            !charter_s_own,
            "charter would ask the operator about a chat charter itself recorded"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_plane_nobody_approved_is_not_vouched_into_consent_by_its_record_being_written() {
        // `vouch` never creates an approval, and this is the wiring mistake that would matter
        // if it did: charter's own bookkeeping would become the operator's yes.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        let records = Records {
            root: root.clone(),
            config: Some(config.clone()),
            allowed: AtomicBool::new(true),
        };

        records.write(&one_chat_on("/bin/true"));

        assert!(would_ask(&config, &root), "a write became an approval");
    }

    /// Where the record lives, which is beside the socket in `.charter/app/`.
    fn record_of(root: &Path) -> PathBuf {
        root.join(".charter").join("app").join("reopen.json")
    }

    #[test]
    fn two_planes_are_held_at_once_each_with_its_own_board() {
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();

        let one = planes.open(&a_plane(&dir.path().join("one")));
        let two = planes.open(&a_plane(&dir.path().join("two")));

        assert_ne!(one, two);
        assert_eq!(planes.open_now().len(), 2);
        // Two boards, not one shared: a chat in one plane is not a chat in the other.
        assert!(
            planes
                .held(&one)
                .expect("held")
                .chats()
                .open_now()
                .is_empty()
                && planes
                    .held(&two)
                    .expect("held")
                    .chats()
                    .open_now()
                    .is_empty()
        );
    }

    #[test]
    fn one_plane_reached_by_two_spellings_is_one_entry_in_the_registry() {
        // Two entries would be two boards and two sockets for one plane on disk — the
        // "acting on the wrong plane" defect, arrived at by a path with a `.` in it.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes();

        let plain = planes.open(&root);
        let roundabout = planes.open(&root.join("."));

        assert_eq!(plain, roundabout);
        assert_eq!(planes.open_now().len(), 1);
    }

    #[test]
    fn closing_a_plane_releases_its_socket_and_leaves_the_plane_on_disk() {
        // Closing a project is the app letting go of it, never the plane going away.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes();
        let plane = planes.open(&root);
        let socket = socket_of(&planes, &plane).expect("a socket was bound");
        assert!(socket.exists(), "nothing was listening to begin with");

        let hooks_were = planes.held(&plane).expect("it is held");
        planes.close(&plane).expect("it closes");

        assert!(planes.open_now().is_empty());
        assert!(
            !hooks_were.hooks().listening(),
            "the thread reading the socket was left running"
        );
        assert!(
            !socket.exists(),
            "the socket outlived the plane that bound it"
        );
        assert!(root.join(charter_core::plane::MANIFEST).is_file());
    }

    #[test]
    fn a_plane_can_be_opened_again_after_it_was_closed() {
        // The socket it left behind is its own, and binding over it is the case
        // `Listener::bind` removes a stale one for.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes();
        let first = planes.open(&root);
        planes.close(&first).expect("it closes");

        let again = planes.open(&root);

        assert_eq!(first, again);
        assert!(socket_of(&planes, &again).is_some_and(|socket| socket.exists()));
    }

    #[test]
    fn closing_a_plane_that_is_not_open_says_which_one() {
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();

        let refused = planes
            .close(&PlaneId::of(dir.path()))
            .expect_err("nothing to close");

        assert!(refused.contains("nothing to close"), "{refused}");
    }

    #[test]
    fn a_launch_outside_every_plane_opens_none_and_is_not_an_error() {
        // The app must come up holding no plane at all and stay useful — that is what the
        // opener attaches to. It used to be a command that answered with an error, and an
        // error is not a state a window can attach anything to.
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();

        let launch = resolving_with(&planes, Ok(dir.path().to_path_buf()), |_| {
            Err("no charter.toml in /tmp or any directory above it".to_owned())
        });

        assert!(launch.plane.is_none());
        assert!(launch.from.is_some(), "the directory it was given is known");
        assert!(launch.why.is_some(), "and why it held no plane");
        assert!(planes.open_now().is_empty());
    }

    #[test]
    fn a_launch_with_no_directory_to_go_on_is_a_different_state_from_one_outside_a_plane() {
        // Told apart by SHAPE and not by reading a sentence: an opener draws "no plane
        // here" for the one and "nothing open yet" for the other, and a window double-
        // clicked from the dock is the second.
        let planes = planes();

        let nothing = resolving_with(
            &planes,
            Err(std::io::Error::other("the directory is gone")),
            |_| panic!("nothing was given to resolve"),
        );

        assert!(nothing.plane.is_none());
        assert!(nothing.from.is_none());
    }

    #[test]
    fn a_launch_inside_a_plane_opens_exactly_that_one_and_says_which() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let under = root.join("workspaces").join("alpha");
        std::fs::create_dir_all(&under).expect("a directory below the plane");
        let planes = planes();

        a_record_naming(&root, "/bin/echo");

        let launch = resolving_with(&planes, Ok(under.clone()), |_| Ok(root.clone()));

        let plane = launch.plane.expect("a plane was opened");
        assert_eq!(launch.why, None);
        assert_eq!(planes.open_now(), vec![plane.clone()]);
        let held = planes.held(&plane).expect("it is held");
        assert_eq!(
            held.root(),
            root.canonicalize().expect("the plane resolves").as_path()
        );
        // And the launch IS the operator's yes, so the record it holds is put back — once
        // they have answered whether they want it back (charter-app#250). Counted rather than
        // looked at: a chat whose program dies at once is a chat that was tried, and on a
        // runner either answer is honest — what must not happen is neither.
        planes.relaunch(Choice::ReopenAll, &[]);
        assert_eq!(
            tried(&held),
            1,
            "the launch attached the plane and never put its record back"
        );
    }

    // ----- the question a relaunch asks (charter-app#250) -----

    /// How many chats a plane has tried to start: running, or tried and refused. Counted
    /// rather than looked at, for the launch test's reason above.
    fn tried(held: &Held) -> usize {
        held.chats().open_now().len() + held.chats().would_not_start().len()
    }

    /// A plane launched in, holding a record of one chat, and the registry that launched it.
    fn launched_with_one_chat(dir: &Path) -> (Planes, PathBuf, Arc<Held>) {
        let root = a_plane(&dir.join("plane"));
        a_record_naming(&root, "/bin/echo");
        let planes = planes();
        let plane = resolving_with(&planes, Ok(root.clone()), |_| Ok(root.clone()))
            .plane
            .expect("a plane was opened");
        let held = planes.held(&plane).expect("it is held");
        (planes, root, held)
    }

    #[test]
    fn a_launch_starts_no_chat_before_the_operator_has_chosen() {
        let dir = tempfile::tempdir().expect("a directory");
        let (_planes, root, held) = launched_with_one_chat(dir.path());

        assert_eq!(tried(&held), 0, "a chat started while the question was up");
        assert!(
            reopen::read_or_refusal(&root)
                .expect("the record reads")
                .holds_anything(),
            "the record was touched before anything was chosen"
        );
    }

    #[test]
    fn a_launch_that_is_never_answered_leaves_the_record_as_it_was_at_the_quit() {
        // A window that never loaded, or a quit with the question still up. The record is
        // not the app's to write until the choice has been made, so nothing is lost.
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, root, _held) = launched_with_one_chat(dir.path());
        let before = std::fs::read(record_of(&root)).expect("the record");

        planes.let_go_of_all();

        assert_eq!(std::fs::read(record_of(&root)).expect("the record"), before);
    }

    #[test]
    fn the_question_names_each_project_with_something_to_put_back_and_what() {
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, root, _held) = launched_with_one_chat(dir.path());
        let other = a_plane(&dir.path().join("other"));
        a_record_naming_each_on(&other, &["/bin/echo", "/bin/echo"]);
        reopen::write(
            &other,
            &reopen::Record {
                views: vec![a_persona_view()],
                ..reopen::read_or_refusal(&other).expect("the record reads")
            },
        )
        .expect("the record is written");
        let empty = a_plane(&dir.path().join("empty"));

        let asked = planes
            .relaunch_ask(&[other.clone(), empty])
            .expect("there is something to ask about");

        let said: Vec<(PathBuf, usize, usize)> = asked
            .projects
            .iter()
            .map(|waiting| (waiting.root.clone(), waiting.chats, waiting.views))
            .collect();
        assert_eq!(
            said,
            vec![
                (root.canonicalize().expect("resolves"), 1, 0),
                (other.canonicalize().expect("resolves"), 2, 1),
            ],
            "the launch's own project first, then each restored one that holds anything"
        );
        assert!(!asked.after_update);
    }

    #[test]
    fn nothing_to_put_back_anywhere_is_no_question() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let other = a_plane(&dir.path().join("other"));
        let planes = planes();
        resolving_with(&planes, Ok(root.clone()), |_| Ok(root.clone()));

        assert!(planes.relaunch_ask(&[other]).is_none());
    }

    #[test]
    fn start_fresh_starts_nothing_and_clears_the_record_but_keeps_every_number_dealt() {
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, root, held) = launched_with_one_chat(dir.path());
        reopen::write(
            &root,
            &reopen::Record {
                dealt: 6,
                ..reopen::read_or_refusal(&root).expect("the record reads")
            },
        )
        .expect("the record is written");

        planes.relaunch(Choice::StartFresh, &[]);

        assert_eq!(tried(&held), 0);
        let after = reopen::read_or_refusal(&root).expect("the record reads");
        assert!(!after.holds_anything(), "the record still holds {after:?}");
        assert_eq!(
            after.dealt, 6,
            "a number already dealt could be dealt again"
        );
    }

    #[test]
    fn the_choice_is_made_once_and_a_reloaded_window_asking_again_changes_nothing() {
        // A webview reload runs the window's launch path a second time. The chats are already
        // back, so a second question — or a second put-back — would be a second copy of each.
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, _root, held) = launched_with_one_chat(dir.path());
        let restored = a_plane(&dir.path().join("restored"));
        a_record_naming(&restored, "/bin/echo");
        let restoring = std::slice::from_ref(&restored);

        planes.relaunch(Choice::ReopenAll, restoring);
        assert!(planes.relaunch_ask(restoring).is_none(), "it asked twice");
        planes.relaunch(Choice::StartFresh, restoring);

        assert_eq!(tried(&held), 1);
        assert!(
            planes.relaunching().fresh.is_empty(),
            "a second answer marked the restore for a fresh start"
        );
    }

    /// Yesterday's session of `root`: approved on this machine, and quit with one chat open.
    ///
    /// The chat is `/bin/cat`, which waits on its terminal until it is ended — so it is still
    /// running when the quit writes the record, and the record the next launch reads names it.
    #[cfg(unix)]
    fn approved_yesterday_with_one_chat(config: &Path, root: &Path) {
        a_record_naming(root, "/bin/cat");
        let planes = planes_keeping(config);
        let shown = asking(planes.open_if_approved(root).expect("it is a plane")).contributes;
        planes.approve_and_open(root, &shown).expect("yes");
        planes.let_go_of_all();
        assert!(
            reopen::read_or_refusal(root)
                .expect("the record reads")
                .holds_anything(),
            "yesterday's quit recorded nothing"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_project_restored_after_start_fresh_opens_with_nothing_put_back() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        approved_yesterday_with_one_chat(&config, &root);

        // Today: the launch restores it, and the operator starts fresh.
        let planes = planes_keeping(&config);
        planes.relaunch(Choice::StartFresh, std::slice::from_ref(&root));
        let plane = opened(planes.open_if_approved(&root).expect("it is a plane"));
        let held = planes.held(&plane).expect("it is held");

        assert_eq!(tried(&held), 0);
        assert!(
            !reopen::read_or_refusal(&root)
                .expect("the record reads")
                .holds_anything()
        );
        // And the cleared record is vouched for, so the next open does not ask about a change
        // charter made itself.
        planes.close(&plane).expect("it closes");
        opened(planes.open_if_approved(&root).expect("it is a plane"));
    }

    #[cfg(unix)]
    #[test]
    fn a_project_opened_after_reopen_all_puts_its_record_back() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        approved_yesterday_with_one_chat(&config, &root);

        let planes = planes_keeping(&config);
        planes.relaunch(Choice::ReopenAll, std::slice::from_ref(&root));
        let plane = opened(planes.open_if_approved(&root).expect("it is a plane"));

        assert_eq!(tried(&planes.held(&plane).expect("it is held")), 1);
    }

    #[test]
    fn a_relaunch_after_an_update_says_so_in_the_question() {
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, root, _held) = launched_with_one_chat(dir.path());
        reopen::write(
            &root,
            &reopen::Record {
                relaunch_after_update: true,
                ..reopen::read_or_refusal(&root).expect("the record reads")
            },
        )
        .expect("the record is written");

        assert!(
            planes
                .relaunch_ask(&[])
                .expect("there is something to ask about")
                .after_update
        );
    }

    fn a_persona_view() -> reopen::View {
        reopen::View {
            from: None,
            view: "persona".to_owned(),
            key: "steward".to_owned(),
            title: "steward".to_owned(),
            workspace: None,
            at: 1,
            active: false,
            pinned: false,
        }
    }

    /// [`a_record_naming_each`], on every platform.
    fn a_record_naming_each_on(root: &Path, programs: &[&str]) {
        let chats = programs
            .iter()
            .enumerate()
            .map(|(which, program)| reopen::Chat {
                name: format!("chat.{which}"),
                ..one_chat_on(program).chats.remove(0)
            })
            .collect();
        reopen::write(
            root,
            &reopen::Record {
                chats,
                ..Default::default()
            },
        )
        .expect("the record is written");
    }

    /// A registry whose machine store is `config`, which is what a real one has.
    #[cfg(unix)]
    fn planes_keeping(config: &Path) -> Planes {
        Planes::telling(
            Arc::new(|_: Moved| {}),
            crate::Shipped::default(),
            Some(config.to_path_buf()),
        )
    }

    /// The `Ask` arm, or a failure naming what came back instead.
    fn asking(opening: Opening) -> Asking {
        match opening {
            Opening::Ask(ask) => ask,
            Opening::Open(plane) => panic!("it opened {} instead of asking", plane.as_str()),
        }
    }

    /// The `Open` arm, or a failure naming what came back instead.
    #[cfg(unix)]
    fn opened(opening: Opening) -> PlaneId {
        match opening {
            Opening::Open(plane) => plane,
            Opening::Ask(ask) => {
                panic!("it asked about {} instead of opening", ask.root.display())
            }
        }
    }

    /// Committed settings that enable one plugin, which is code inside the operator's harness.
    #[cfg(unix)]
    fn enabling_a_plugin(root: &Path, name: &str) {
        let claude = root.join(".claude");
        std::fs::create_dir_all(&claude).expect("the plane's .claude");
        std::fs::write(
            claude.join("settings.json"),
            format!("{{\"enabledPlugins\": {{\"{name}\": true}}}}"),
        )
        .expect("the plane's settings");
    }

    #[cfg(unix)]
    #[test]
    fn a_plane_nobody_has_approved_is_described_rather_than_opened() {
        // The whole gate, from the outside: `.charter/app/reopen.json` is an execution input,
        // and a directory arrives by zip as readily as by clone. Nothing may be attached and
        // nothing may be started until the operator has read what opening it puts in force.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        a_record_naming(&root, "/bin/echo");
        enabling_a_plugin(&root, "stranger@market");
        let planes = planes_keeping(&config);

        let asked = asking(planes.open_if_approved(&root).expect("it is a plane"));

        assert!(
            planes.open_now().is_empty(),
            "an ask attached the plane; a cancelled dialog would leave a socket bound in it"
        );
        assert!(asked.first(), "a plane nobody has approved is a first ask");
        assert!(
            asked.contributes.plugins.contains_key("stranger@market"),
            "the ask did not say which plugins the plane enables"
        );
        assert_eq!(
            asked.contributes.starts.len(),
            1,
            "the ask did not say what the plane would start"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_operator_s_yes_is_what_opens_a_plane_and_puts_its_record_back() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        a_record_naming(&root, "/bin/echo");
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;

        let plane = planes
            .approve_and_open(&root, &shown)
            .expect("the operator said yes");

        let held = planes.held(&plane).expect("it is held");
        // Counted rather than looked at, as the launch's own test does: a program that dies
        // at once is a chat that was tried, and on a runner either answer is honest.
        assert_eq!(
            held.chats().open_now().len() + held.chats().would_not_start().len(),
            1,
            "the yes opened the plane and never put its record back"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_plane_the_operator_already_approved_opens_without_asking_again() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        enabling_a_plugin(&root, "known@market");
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;
        let first = planes
            .approve_and_open(&root, &shown)
            .expect("the operator said yes");
        planes.close(&first).expect("it closes");

        let again = opened(planes.open_if_approved(&root).expect("it is a plane"));

        assert_eq!(first, again);
    }

    #[cfg(unix)]
    #[test]
    fn opening_a_plane_that_is_already_open_does_not_put_its_record_back_a_second_time() {
        // `Planes::open` hands back the id a plane already has and binds nothing twice;
        // `reopen` has no such rule, because at a launch there is nothing to have put back
        // yet. So a recents row clicked twice — or a second launch naming the project already
        // on screen — would start a second copy of every chat the record names, beside the
        // copies already running, and the operator would have no way to tell which was which.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        a_record_naming(&root, "/bin/echo");
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;
        let plane = planes.approve_and_open(&root, &shown).expect("yes");
        let held = planes.held(&plane).expect("it is held");
        let once = held.chats().open_now().len() + held.chats().would_not_start().len();
        assert_eq!(once, 1, "the yes did not put the record back at all");

        let again = opened(planes.open_if_approved(&root).expect("it is a plane"));

        assert_eq!(plane, again);
        assert_eq!(
            held.chats().open_now().len() + held.chats().would_not_start().len(),
            once,
            "opening a plane that was already open started its chats again"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_plane_that_is_already_open_is_shown_rather_than_asked_about_when_it_changes() {
        // "Open it" for a project already on screen means "show me that project" — a recents
        // row, or a second launch naming it. A dialog there would be in front of chats that
        // are already running, about a grant that is already in force, and there is nothing
        // the operator could answer that would undo either. That is the prompt that teaches
        // them to click yes without reading, which is the failure the whole ask exists to
        // avoid paying for.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;
        let plane = planes.approve_and_open(&root, &shown).expect("yes");

        // The kind of change that WOULD re-ask about a plane that was not open.
        enabling_a_plugin(&root, "arrived@market");

        let again = opened(planes.open_if_approved(&root).expect("it is a plane"));
        assert_eq!(plane, again);
    }

    #[cfg(unix)]
    #[test]
    fn a_plane_that_gained_a_plugin_since_it_was_approved_asks_again_and_says_what_is_new() {
        // `enabledPlugins` is code that will run inside the operator's harness, and it
        // travels out of the plane's COMMITTED settings — so an ordinary `git pull` of a
        // shared plane can hand over a grant nobody looked at.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;
        let plane = planes.approve_and_open(&root, &shown).expect("yes");
        planes.close(&plane).expect("it closes");

        enabling_a_plugin(&root, "new@market");
        let asked = asking(planes.open_if_approved(&root).expect("it is a plane"));

        assert!(!asked.first(), "a re-ask was drawn as a first approval");
        assert!(
            asked
                .consent
                .changes()
                .iter()
                .any(|change| change.name() == "new@market"),
            "the re-ask did not name the plugin that appeared: {:?}",
            asked.consent.changes()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_plane_that_only_stopped_contributing_something_is_opened_rather_than_asked_about() {
        // `Consent::Noted`. A withdrawal cannot make anything run that the approval did not
        // already cover, and a prompt that never carries risk is one an operator learns to
        // answer yes to without reading.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        enabling_a_plugin(&root, "going@market");
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;
        let plane = planes.approve_and_open(&root, &shown).expect("yes");
        planes.close(&plane).expect("it closes");

        std::fs::write(root.join(".claude").join("settings.json"), "{}").expect("the plugin goes");

        let again = opened(planes.open_if_approved(&root).expect("it is a plane"));
        assert_eq!(plane, again);
    }

    #[cfg(unix)]
    #[test]
    fn a_plane_that_changed_between_the_ask_and_the_click_approves_nothing_and_opens_nothing() {
        // Between the dialog reading the plane and the operator pressing the button, anything
        // on the machine — including a chat running in another plane — can rewrite this
        // plane's settings or its reopen record. Without this check the approval would record
        // whatever was on disk at CLICK time, so the operator could approve, and charter could
        // then start, a program they never read.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;

        enabling_a_plugin(&root, "slipped-in@market");
        let refused = planes
            .approve_and_open(&root, &shown)
            .expect_err("it must refuse an approval of something else");

        assert!(
            refused.contains("changed while you were reading it"),
            "{refused}"
        );
        assert!(planes.open_now().is_empty(), "it opened the plane anyway");
        // And nothing was written down either, so the next ask is still a first ask.
        assert!(
            asking(planes.open_if_approved(&root).expect("it is a plane")).first(),
            "a refused approval was recorded"
        );
    }

    #[cfg(unix)]
    #[test]
    fn opening_a_plane_puts_it_in_the_list_the_opener_offers() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes_keeping(&config);
        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;

        planes.approve_and_open(&root, &shown).expect("yes");

        let remembered = planes.remembered().store;
        let entry = remembered
            .recent(&root.canonicalize().expect("the plane resolves"))
            .expect("the plane it just opened is in the list");
        assert!(entry.trust.is_some(), "the yes was not written down");
    }

    #[cfg(unix)]
    #[test]
    fn a_launch_s_own_plane_is_remembered_so_the_opener_has_something_to_offer() {
        // An operator who has only ever run charter from a terminal must not find the opener
        // empty the first time they double-click the icon. Remembered, and deliberately NOT
        // approved: running charter here is consent to open it now, and the recorded approval
        // is an answer to a question they were shown.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes_keeping(&config);

        resolving_with(&planes, Ok(root.clone()), |_| Ok(root.clone()));

        let resolved = root.canonicalize().expect("the plane resolves");
        let entry = planes
            .remembered()
            .store
            .recent(&resolved)
            .cloned()
            .expect("the launch's plane is in the list");
        assert!(
            entry.trust.is_none(),
            "a terminal launch became a recorded approval"
        );
    }

    #[test]
    fn a_machine_with_no_store_asks_every_time_and_still_opens_on_the_answer() {
        // Windows keeps no store at all (ADR 0031), so no answer can be written down. The
        // gate still bites — every open is asked about — and the app is still an app.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes();

        let shown = asking(planes.open_if_approved(&root).expect("it is a plane")).contributes;
        let plane = planes.approve_and_open(&root, &shown).expect("yes");
        planes.close(&plane).expect("it closes");

        assert!(
            asking(planes.open_if_approved(&root).expect("it is a plane")).first(),
            "a machine that cannot remember an approval behaved as though it had one"
        );
    }

    #[test]
    fn a_directory_inside_a_plane_opens_the_plane_above_it_and_says_which() {
        // A picker pointed at `workspaces/alpha` means the project, and the approval is
        // recorded against the root — which is why the ask carries the root it resolved
        // rather than the path that was handed in.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        let under = root.join("workspaces").join("alpha");
        std::fs::create_dir_all(&under).expect("a directory below the plane");
        let planes = planes();

        let asked = asking(
            planes
                .open_if_approved(&under)
                .expect("it is under a plane"),
        );

        assert_eq!(asked.root, root.canonicalize().expect("the plane resolves"));
    }

    #[test]
    fn a_directory_that_is_in_no_plane_is_refused_by_name_rather_than_opened_as_one() {
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();

        let refused = planes
            .open_if_approved(dir.path())
            .map(|_| ())
            .expect_err("nothing there is a plane");

        assert!(refused.contains(charter_core::plane::MANIFEST), "{refused}");
    }

    #[test]
    fn a_path_that_is_not_there_at_all_says_so_rather_than_being_treated_as_empty() {
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();

        let refused = planes
            .open_if_approved(&dir.path().join("never"))
            .map(|_| ())
            .expect_err("there is nothing at that path");

        assert!(refused.contains("cannot open"), "{refused}");
    }

    #[test]
    fn a_window_showing_one_plane_is_not_looking_at_another_plane_s_chat() {
        // The half #111 named as missing: every plane numbers its chats from one, so "is
        // session 3 in front" has as many answers as there are planes open, and a window
        // showing B would have suppressed a notification for A's chat 3.
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();
        let one = planes.open(&a_plane(&dir.path().join("one")));
        let two = planes.open(&a_plane(&dir.path().join("two")));
        let showing = Showing::default();

        showing.in_window(
            "main",
            Holding {
                planes: vec![one.clone(), two.clone()],
                active: Some(0),
            },
        );

        assert!(showing.is_showing("main", &one));
        // **And this is what project tabs changed about the question.** The window HOLDS
        // plane two — fifty chats of it can be live in the tab behind the one on screen —
        // and a notification about one of them is exactly the notification the operator
        // needs. "Open in this window" is not "in front".
        assert!(!showing.is_showing("main", &two));
        // And a window that has never said is not looking at anything, so nothing is
        // suppressed on the strength of silence.
        assert!(!showing.is_showing("second", &one));
        // The last project closed: the window is showing no plane at all.
        showing.in_window("main", Holding::default());
        assert!(!showing.is_showing("main", &one));
    }

    #[test]
    fn a_window_on_its_opener_is_looking_at_none_of_the_planes_it_holds() {
        // The operator pressed `+` to open another project. Both projects are still held and
        // still running; neither is on screen, so neither suppresses anything.
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();
        let one = planes.open(&a_plane(&dir.path().join("one")));
        let two = planes.open(&a_plane(&dir.path().join("two")));
        let showing = Showing::default();

        showing.in_window(
            "main",
            Holding {
                planes: vec![one.clone(), two.clone()],
                active: None,
            },
        );

        assert!(!showing.is_showing("main", &one));
        assert!(!showing.is_showing("main", &two));
    }

    #[cfg(unix)]
    #[test]
    fn what_a_window_holds_is_written_down_so_the_next_launch_can_put_it_back() {
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let one = a_plane(&dir.path().join("one"));
        let two = a_plane(&dir.path().join("two"));
        let planes = planes_keeping(&config);
        let first = planes.open(&one);
        let second = planes.open(&two);

        planes.remember_arrangement(&[Holding {
            planes: vec![first, second],
            active: Some(1),
        }]);

        let back = restorable(machine::read(&config));
        assert_eq!(
            back.planes,
            vec![
                one.canonicalize().expect("one resolves"),
                two.canonicalize().expect("two resolves")
            ]
        );
        assert_eq!(back.active, Some(1), "the tab that was in front is not");
        assert!(back.dropped.is_empty(), "{:?}", back.dropped);
    }

    #[cfg(unix)]
    #[test]
    fn a_window_that_holds_nothing_writes_an_arrangement_with_nothing_in_it() {
        // The operator closed the last project and quit. Coming back to the tabs they had
        // just closed would be charter overruling them with a file.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let root = a_plane(&dir.path().join("plane"));
        let planes = planes_keeping(&config);
        let plane = planes.open(&root);
        planes.remember_arrangement(&[Holding {
            planes: vec![plane],
            active: Some(0),
        }]);
        assert_eq!(restorable(machine::read(&config)).planes.len(), 1);

        planes.remember_arrangement(&[]);

        assert!(restorable(machine::read(&config)).planes.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_plane_the_registry_has_let_go_of_takes_its_tab_out_and_the_front_one_moves_with_it() {
        // The id is the only thing a window sends, and an id the process is no longer holding
        // cannot be turned back into a root honestly. Finding the front tab again in the list
        // that SURVIVED is how a window comes back on the project it was on.
        //
        // **Three projects, the first let go of, and the second in front.** Two would prove
        // nothing: `machine::read` clamps an index past the end, so a carried-over `active`
        // happens to land on the right project whenever the tab that went was before it and
        // the front one was last. Here the clamp cannot save it — a carried-over `1` is `c`,
        // and the operator was on `b`.
        let dir = tempfile::tempdir().expect("a directory");
        let config = dir.path().join("config");
        let a = a_plane(&dir.path().join("a"));
        let b = a_plane(&dir.path().join("b"));
        let c = a_plane(&dir.path().join("c"));
        let planes = planes_keeping(&config);
        let first = planes.open(&a);
        let second = planes.open(&b);
        let third = planes.open(&c);
        planes.close(&first).expect("the first project closes");

        planes.remember_arrangement(&[Holding {
            planes: vec![first, second, third],
            active: Some(1),
        }]);

        let back = restorable(machine::read(&config));
        assert_eq!(
            back.planes,
            vec![
                b.canonicalize().expect("b resolves"),
                c.canonicalize().expect("c resolves")
            ]
        );
        assert_eq!(
            back.active,
            Some(0),
            "the window came back on the wrong project"
        );
    }

    #[test]
    fn a_window_that_lets_go_of_its_last_project_stops_being_a_window_in_the_arrangement() {
        // An empty window in the arrangement is a row charter writes down and then restores
        // as nothing. It is also the shape that would make "the operator closed everything"
        // and "there is a window here with nothing in it" indistinguishable.
        let dir = tempfile::tempdir().expect("a directory");
        let planes = planes();
        let one = planes.open(&a_plane(&dir.path().join("one")));
        let showing = Showing::default();
        showing.in_window(
            "main",
            Holding {
                planes: vec![one],
                active: Some(0),
            },
        );
        assert_eq!(showing.arrangement().len(), 1);

        showing.in_window("main", Holding::default());

        assert!(showing.arrangement().is_empty());
    }

    #[test]
    fn a_remembered_project_that_has_gone_is_dropped_with_a_line_and_never_an_error() {
        // ADR 0033: a restore is a convenience, and a convenience that blocks the launch is
        // worse than the thing it was restoring. It is `put_back`'s rule for a chat whose
        // profile has gone, one scope up.
        let dir = tempfile::tempdir().expect("a directory");
        let there = a_plane(&dir.path().join("there"));
        let gone = dir.path().join("gone");
        let store = machine::Store {
            windows: vec![machine::Window {
                planes: vec![gone.clone(), there.clone()],
                active: 0,
            }],
            ..machine::Store::default()
        };

        let back = restorable(machine::Loaded {
            store,
            ..machine::Loaded::default()
        });

        assert_eq!(back.planes, vec![there]);
        assert_eq!(back.dropped.len(), 1, "{:?}", back.dropped);
        assert!(
            back.dropped[0].contains("no longer there"),
            "{:?}",
            back.dropped
        );
        // The tab that was in front is the one that went, so the window comes back on the
        // project that survived rather than on an opener in front of it.
        assert_eq!(back.active, Some(0));
    }

    #[test]
    fn every_remembered_window_merges_into_the_one_window_this_app_can_draw() {
        // Splitting a tab back out into its own OS window needs multi-window Tauri and is not
        // here yet. Dropping the second window's tabs would lose the operator projects to a
        // limitation they never asked for, so they merge — and a project both windows held is
        // one tab, because two tabs on one plane would be two views of one board.
        let dir = tempfile::tempdir().expect("a directory");
        let one = a_plane(&dir.path().join("one"));
        let two = a_plane(&dir.path().join("two"));
        let store = machine::Store {
            windows: vec![
                machine::Window {
                    planes: vec![one.clone()],
                    active: 0,
                },
                machine::Window {
                    planes: vec![two.clone(), one.clone()],
                    active: 0,
                },
            ],
            ..machine::Store::default()
        };

        let back = restorable(machine::Loaded {
            store,
            ..machine::Loaded::default()
        });

        assert_eq!(back.planes, vec![one, two]);
        assert_eq!(back.active, Some(0), "the first window's front tab is lost");
    }

    #[test]
    fn a_launch_told_not_to_restore_is_told_by_its_own_arguments_and_nothing_else() {
        assert!(Restoring::from_args(["charter-app".to_owned()]).wanted());
        assert!(
            !Restoring::from_args(["charter-app".to_owned(), NO_RESTORE.to_owned()]).wanted(),
            "--no-restore was read as a request to restore"
        );
        // Not a prefix and not a substring: `--no-restore-really` is not this flag, and a
        // window that treated it as one would silently throw away the operator's tabs.
        assert!(
            Restoring::from_args(["charter-app".to_owned(), "--no-restore-really".to_owned()])
                .wanted()
        );
    }

    // **A needs-you item never outlives its chat (charter-app#247).** The window's queue is
    // whatever the LAST `chat-moved` it was sent carried (`chatState.ts:moved`), so every way
    // a chat can stop being open has to end with the window being told a queue without it.

    /// A registry that keeps everything it would have told a window.
    fn planes_telling() -> (Planes, Arc<Mutex<Vec<Moved>>>) {
        let told = Arc::new(Mutex::new(Vec::new()));
        let tell = Arc::clone(&told);
        let planes = Planes::telling(
            Arc::new(move |moved: Moved| tell.lock().expect("the log").push(moved)),
            crate::Shipped::default(),
            None,
        );
        (planes, told)
    }

    /// The needs-you queue a window showing `plane` holds now: the newest one it was sent,
    /// which is the one numbered last — the window drops a snapshot older than the one it
    /// holds, whatever order they land in (`chatState.ts`, charter-app#248).
    fn the_window_s_queue(told: &Mutex<Vec<Moved>>, plane: &PlaneId) -> Vec<u32> {
        told.lock()
            .expect("the log")
            .iter()
            .filter(|moved| moved.plane == *plane)
            .max_by_key(|moved| moved.sequence)
            .map(|moved| moved.queue.clone())
            .unwrap_or_default()
    }

    /// Waits, up to a bound, for the window's queue to satisfy `wanted`.
    fn the_window_s_queue_becomes(
        told: &Mutex<Vec<Moved>>,
        plane: &PlaneId,
        wanted: impl Fn(&[u32]) -> bool,
    ) -> Vec<u32> {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let queue = the_window_s_queue(told, plane);
            if wanted(&queue) || std::time::Instant::now() > until {
                return queue;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// A chat on `program` that has stopped and asked for the operator, over the real socket.
    fn a_chat_asking_for_you(held: &Held, told: &Mutex<Vec<Moved>>, program: &str) -> u32 {
        let session = held
            .chats()
            .start(&one_chat_on(program).chats[0], STARTING)
            .expect("the chat starts");
        a_stop_from(held, session);
        assert_eq!(
            the_window_s_queue_becomes(told, &held.id, |queue| queue.contains(&session)),
            vec![session],
            "the chat never asked for you, so nothing below is evidence"
        );
        session
    }

    /// The `Stop` a harness's hook sends when its turn ends.
    fn a_stop_from(held: &Held, session: u32) {
        charter_core::hookwire::send(
            held.hooks().socket().expect("the plane is listening"),
            &charter_core::hookwire::Report {
                chat: session,
                event: charter_core::state::Event::Stop,
                conversation: charter_core::hookwire::Conversation::Unknown,
                pid: None,
                detail: charter_core::state::Detail::default(),
            },
        )
        .expect("the hook reaches the plane");
    }

    #[cfg(unix)]
    #[test]
    fn closing_a_chat_that_needs_you_takes_it_out_of_the_window_s_queue() {
        // The tab's ×, and ending the pane: both are `close_session`, which is this.
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, told) = planes_telling();
        let plane = planes.open(&a_plane(&dir.path().join("plane")));
        let held = planes.held(&plane).expect("it is held");
        let session = a_chat_asking_for_you(&held, &told, "/bin/cat");

        held.close_chat(session).expect("it closes");

        assert_eq!(
            the_window_s_queue(&told, &plane),
            Vec::<u32>::new(),
            "the chat is closed and the window still shows it needing you"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_chat_whose_program_ends_by_itself_leaves_the_window_s_queue() {
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, told) = planes_telling();
        let plane = planes.open(&a_plane(&dir.path().join("plane")));
        let held = planes.held(&plane).expect("it is held");
        let session = a_chat_asking_for_you(&held, &told, "/bin/cat");

        // End of input: `cat` exits 0 on its own, as a harness does on `/exit`.
        held.chats()
            .sessions()
            .input(session, "\u{4}")
            .expect("the chat takes input");

        assert_eq!(
            the_window_s_queue_becomes(&told, &plane, <[u32]>::is_empty),
            Vec::<u32>::new(),
            "the chat's program has ended and the window still shows it needing you"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_relaunch_never_puts_back_a_chat_needing_you() {
        // Quit with a chat asking, then launch again: the record puts the chat back, and the
        // chat has asked nothing of THIS run.
        let dir = tempfile::tempdir().expect("a directory");
        let root = a_plane(&dir.path().join("plane"));
        {
            let (planes, told) = planes_telling();
            let plane = planes.open(&root);
            let held = planes.held(&plane).expect("it is held");
            held.reopen(STARTING, Ok(reopen::Record::default()));
            a_chat_asking_for_you(&held, &told, "/bin/cat");

            planes.let_go_of_all();

            assert_eq!(
                the_window_s_queue_becomes(&told, &plane, <[u32]>::is_empty),
                Vec::<u32>::new(),
                "the quit ended the chat and the window still shows it needing you"
            );
        }

        let (planes, told) = planes_telling();
        let plane = planes.open(&root);
        let held = planes.held(&plane).expect("it is held");
        held.reopen(STARTING, reopen::read_or_refusal(&root));
        let back = held.chats().open_now();
        assert_eq!(back.len(), 1, "the record did not put the chat back");

        // What a window opening now is answered (`chat_states`), and what it has been told.
        let snapshot: Vec<Moved> = back
            .iter()
            .map(|open| held.hooks().now(open.session))
            .collect();
        assert!(
            snapshot
                .iter()
                .all(|moved| moved.queue.is_empty() && !moved.needs_you),
            "a relaunch put back a chat needing you: {snapshot:?}"
        );
        let told = told.lock().expect("the log");
        assert!(
            told.iter()
                .all(|moved| moved.queue.is_empty() && !moved.needs_you),
            "a relaunch told the window a chat needs you: {told:?}"
        );
    }

    // **Ignore lasts until the chat asks again (charter-app#248).**

    #[cfg(unix)]
    #[test]
    fn ignoring_a_chat_that_needs_you_takes_it_out_of_the_window_s_queue() {
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, told) = planes_telling();
        let plane = planes.open(&a_plane(&dir.path().join("plane")));
        let held = planes.held(&plane).expect("it is held");
        let session = a_chat_asking_for_you(&held, &told, "/bin/cat");

        held.ignore_needs_you(session);

        assert_eq!(
            the_window_s_queue(&told, &plane),
            Vec::<u32>::new(),
            "the chat was ignored and the window still shows it needing you"
        );
        assert_eq!(
            held.hooks().now(session).state,
            "waiting",
            "ignoring a chat answered nothing in it"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_ignored_chat_that_stops_again_is_back_in_the_window_s_queue() {
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, told) = planes_telling();
        let plane = planes.open(&a_plane(&dir.path().join("plane")));
        let held = planes.held(&plane).expect("it is held");
        let session = a_chat_asking_for_you(&held, &told, "/bin/cat");
        held.ignore_needs_you(session);
        assert!(the_window_s_queue(&told, &plane).is_empty());

        a_stop_from(&held, session);

        assert_eq!(
            the_window_s_queue_becomes(&told, &plane, |queue| queue.contains(&session)),
            vec![session],
            "the chat asked again and the window was not told"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_ignored_chat_is_still_ignored_when_a_window_asks_again() {
        // A window reload asks `chat_states`, and the board is what answers it.
        let dir = tempfile::tempdir().expect("a directory");
        let (planes, told) = planes_telling();
        let plane = planes.open(&a_plane(&dir.path().join("plane")));
        let held = planes.held(&plane).expect("it is held");
        let session = a_chat_asking_for_you(&held, &told, "/bin/cat");

        held.ignore_needs_you(session);

        let asked = held.hooks().now(session);
        assert!(asked.queue.is_empty() && !asked.needs_you, "{asked:?}");
    }
}
