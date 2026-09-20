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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use charter_core::engine::Size;
use charter_core::reopen;

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
/// **The one yes this module can honestly mint is the launch's own working directory** —
/// the operator ran charter there, which is the act. Every other way in (an opener handing
/// over a path, a recents list, a second instance's argument) has to get its yes from the
/// trust gate, which is a separate piece of work; adding a second way to mint one is then a
/// deliberate edit to this file rather than an argument somebody forgot to pass.
pub struct Approved(());

/// What the app holds for one open plane: its board, its chats, and where it is.
pub struct Held {
    id: PlaneId,
    root: PathBuf,
    hooks: Hooks,
    chats: Chats,
    /// Whether this plane's record is the app's to read and write.
    ///
    /// Set when the record is put back, which only an approved plane reaches. An attached
    /// plane nobody has approved is not recorded EITHER WAY: reading it would run what it
    /// names, and writing it would replace the operator's own record — of a plane they never
    /// said yes to — with whatever this process happens to have open, which is nothing.
    records: Arc<AtomicBool>,
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
        if !self.records.load(Ordering::SeqCst) {
            return;
        }
        if let Err(why) = reopen::write(&self.root, &self.chats.record()) {
            eprintln!(
                "charter: what was open in {} was not recorded ({why})",
                self.root.display()
            );
        }
    }

    /// Puts back the chats this plane had open when it was last closed, which STARTS the
    /// programs its record names.
    ///
    /// Said out loud, on standard error, because an operator whose chats did not come back
    /// otherwise has nothing at all to look at — and neither does a CI log.
    ///
    /// The plane starts recording HERE, and not before: from this moment the app has read
    /// what was there, so writing over it is replacing its own answer rather than the
    /// operator's. It is set before the record is read, so a chat that starts during the
    /// reopen is recorded like any other.
    fn reopen(&self, size: Size) {
        self.records.store(true, Ordering::SeqCst);
        let record = match reopen::read_or_refusal(&self.root) {
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

    /// Writes the record, ends every session, and stops listening — everything a plane holds
    /// in this process, and nothing it has on disk.
    ///
    /// The record is written BEFORE the sessions are ended, because ending them is what makes
    /// there be nothing to write.
    fn let_go(&self) {
        self.record();
        self.chats.end_all();
        self.hooks.stop();
    }
}

/// Told whenever a chat moves, whichever plane it is in. The event carries its plane, so one
/// teller serves them all.
pub type Teller = Arc<dyn Fn(Moved) + Send + Sync + 'static>;

/// The planes this process holds, by root.
pub struct Planes {
    tell: Teller,
    /// The `charter` binary a hook runs, where the app found one. Every plane arms with the
    /// same one: it is a property of this build, not of a project.
    binary: Option<PathBuf>,
    open: Mutex<HashMap<PlaneId, Arc<Held>>>,
}

impl Planes {
    /// A registry holding nothing, which is what the app comes up as before a plane is
    /// opened — and stays as, perfectly happily, when there is no plane to open.
    pub fn telling(tell: Teller, binary: Option<PathBuf>) -> Self {
        Self {
            tell,
            binary,
            open: Mutex::new(HashMap::new()),
        }
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
        let id = PlaneId::of(&root);

        let mut open = self.map();
        if let Some(already) = open.get(&id) {
            return already.id.clone();
        }
        let held = Arc::new(self.hold(id.clone(), root));
        open.insert(id.clone(), Arc::clone(&held));
        id
    }

    /// Puts back what the plane held, which **starts the programs its record names**.
    ///
    /// Private, and it takes [`Approved`] — a value nothing outside this module can build.
    /// That is the gate: [`Planes::open`] above attaches a plane and has no way to reach
    /// this, so a plane the operator has not said yes to cannot run anything.
    fn reopen(&self, plane: &PlaneId, _yes: &Approved) {
        // The handle is taken and the lock dropped BEFORE anything starts: reopening runs
        // programs, and a program that dies at once tells the board, which tells the window,
        // which asks this very registry what the chat is called.
        let Ok(held) = self.held(plane) else { return };
        held.reopen(STARTING);
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

        let records = Arc::new(AtomicBool::new(false));
        let writing_to = root.clone();
        let may_record = Arc::clone(&records);
        let mut chats = Chats::recorded_by_reporting_to(
            Box::new(move |record| {
                // An attached plane nobody approved writes nothing: its record is the
                // operator's, and this process has not read it.
                if !may_record.load(Ordering::SeqCst) {
                    return;
                }
                // A record that cannot be written is not worth interrupting the operator
                // over, but it is worth saying: a refused record means every later launch of
                // this plane comes back empty, and silence makes that look like a plane that
                // never had chats in it.
                if let Err(why) = reopen::write(&writing_to, record) {
                    eprintln!("charter: what is open was not recorded ({why})");
                }
            }),
            reporting,
        );
        chats.arming_with(self.binary.clone());

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

        Held {
            id,
            root,
            hooks,
            chats,
            records,
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
            // **The one approval this module mints.** The operator ran charter in this
            // directory; that act is the yes, and it is the yes for THIS plane and no other.
            planes.reopen(&plane, &Approved(()));
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
        Planes::telling(Arc::new(|_: Moved| {}), None)
    }

    /// A plane on disk, with nothing in it but the marker that makes it one.
    fn a_plane(at: &Path) -> PathBuf {
        std::fs::create_dir_all(at).expect("the plane's directory");
        std::fs::write(at.join(charter_core::plane::MANIFEST), "").expect("its charter.toml");
        at.to_path_buf()
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

    /// A record in `root` naming one chat on `program`, and the bytes it left on disk.
    fn a_record_naming(root: &Path, program: &str) -> Vec<u8> {
        let record = reopen::Record {
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
            }],
        };
        reopen::write(root, &record).expect("the record is written");
        std::fs::read(record_of(root)).expect("the record reads back")
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
        // And the launch IS the operator's yes, so the record it holds is put back. Counted
        // rather than looked at: a chat whose program dies at once is a chat that was tried,
        // and on a runner either answer is honest — what must not happen is neither.
        assert_eq!(
            held.chats().open_now().len() + held.chats().would_not_start().len(),
            1,
            "the launch attached the plane and never put its record back"
        );
    }
}
