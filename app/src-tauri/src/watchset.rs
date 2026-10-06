//! What the app's file watchers share (`planewatch.rs`, `filewatch.rs`, `branchwatch.rs`,
//! `killswitch.rs`):
//! which events are changes, folding the platform's events into bursts, and keeping a
//! watcher's watches equal to a wanted set of folders, each watched non-recursively.
//!
//! **Every watcher folds the platform's own events, here, and none uses a debouncer**
//! (#1138, #1139). `notify-debouncer-full` takes a file removed while its creation is still
//! being folded as never having been there and reports neither, so a short-lived file — one an
//! agent writes and removes, a stop marker taken away at once — went unheard. And it debounces
//! each path on its own clock, ticked a quarter of its timeout apart, so writes spread over
//! more than a tick by a busy machine came out as several batches rather than one. A burst
//! here is every change from its first event until [`bursts`]' `quiet_for` after it, whatever
//! its paths did in between.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use notify::RecursiveMode;
use notify::event::{CreateKind, EventKind, MetadataKind, ModifyKind};

/// Whether an event is a change. Everything but an access is: reading a file changes nothing,
/// and it is exactly what the window does when it is told. An access time moving is the same
/// read seen from inotify's `IN_ATTRIB` under `relatime`. Without this, a window that re-read on
/// every event would re-read because it re-read, forever.
pub(crate) fn matters(kind: &EventKind) -> bool {
    !matches!(
        kind,
        EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime))
    )
}

/// The handler a platform watcher is made with: every event that is a change, sent on to
/// [`bursts`].
///
/// **An error fails closed**: the watcher could not say what changed, so it is sent on as the
/// platform losing track (a rescan), and its burst is everything. Dropped, as the debouncer
/// before it dropped one, it was a change nobody was ever told of.
pub(crate) fn sender(events: Sender<notify::Event>) -> impl notify::EventHandler {
    move |event: notify::Result<notify::Event>| {
        let event = match event {
            Ok(event) if matters(&event.kind) => event,
            Ok(_) => return,
            Err(_) => notify::Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan),
        };
        // The receiving half is gone only once its watch is.
        let _ = events.send(event);
    }
}

/// One burst of changes, folded.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Burst {
    /// Every path a change named. Empty when [`Burst::everything`].
    pub paths: HashSet<PathBuf>,
    /// The paths among them a removal named.
    pub removed: HashSet<PathBuf>,
    /// Anything may have changed: the platform lost track, an event named no path, or the
    /// burst named more paths than it holds.
    pub everything: bool,
    /// A folder may have been made: a creation the platform named as a folder's, or as
    /// anything's, or a burst that is everything.
    pub made_folder: bool,
    /// The platform lost track or its watcher erred (a rescan, or an event naming no path),
    /// rather than the burst holding more paths than it can: what is watched may no longer be
    /// heard, and a watch that vouched for a whole tree no longer does (FD-11).
    pub lost: bool,
}

impl Burst {
    /// Folds `event` in, holding at most `most` paths: past that the burst counts as
    /// everything, so a flood costs a fixed amount of memory and the reader reads a little
    /// more than it needed to rather than missing a change.
    fn add(&mut self, event: notify::Event, most: usize) {
        if self.everything {
            // Already everything, by overflow perhaps: a loss after it is still a loss.
            self.lost |= event.need_rescan() || event.paths.is_empty();
            return;
        }
        if event.need_rescan() || event.paths.is_empty() {
            self.lost = true;
            return self.is_everything();
        }
        let removed = matches!(event.kind, EventKind::Remove(_));
        self.made_folder |= matches!(
            event.kind,
            EventKind::Create(CreateKind::Folder | CreateKind::Any)
        );
        for path in event.paths {
            if self.paths.len() >= most && !self.paths.contains(&path) {
                return self.is_everything();
            }
            if removed {
                self.removed.insert(path.clone());
            }
            self.paths.insert(path);
        }
    }

    fn is_everything(&mut self) {
        self.everything = true;
        self.made_folder = true;
        self.paths = HashSet::new();
        self.removed = HashSet::new();
    }
}

/// The changes `events` carries, one [`Burst`] at a time: each from its first event until
/// `quiet_for` after it, holding at most `most` paths. It ends when the watcher sending them is
/// dropped, and a burst cut short by that is not handed on: nobody is listening for it.
///
/// **The deadline is checked before every receive**, so a flood still ends its burst on time:
/// `recv_timeout` with no time left answers with what is queued and never times out, and a
/// burst that ended only on a quiet moment never ended under an agent writing without pause.
pub(crate) fn bursts(
    events: Receiver<notify::Event>,
    quiet_for: Duration,
    most: usize,
) -> impl Iterator<Item = Burst> {
    std::iter::from_fn(move || {
        let first = events.recv().ok()?;
        rest_of_burst(&events, first, quiet_for, most)
    })
}

/// [`bursts`], and `None` each time `idle` passes with no event at all: for a watch that also
/// looks for itself now and then, so a watch the platform has quietly lost makes it slow rather
/// than blind. It ends as [`bursts`] does.
pub(crate) fn bursts_or_idle(
    events: Receiver<notify::Event>,
    quiet_for: Duration,
    most: usize,
    idle: Duration,
) -> impl Iterator<Item = Option<Burst>> {
    std::iter::from_fn(move || match events.recv_timeout(idle) {
        Ok(first) => rest_of_burst(&events, first, quiet_for, most).map(Some),
        Err(RecvTimeoutError::Timeout) => Some(None),
        Err(RecvTimeoutError::Disconnected) => None,
    })
}

/// The burst `first` begins: everything received until `quiet_for` after it, or `None` when the
/// watcher is dropped before then.
fn rest_of_burst(
    events: &Receiver<notify::Event>,
    first: notify::Event,
    quiet_for: Duration,
    most: usize,
) -> Option<Burst> {
    let mut burst = Burst::default();
    burst.add(first, most);
    let until = Instant::now() + quiet_for;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Some(burst);
        }
        match events.recv_timeout(left) {
            Ok(more) => burst.add(more, most),
            Err(RecvTimeoutError::Timeout) => return Some(burst),
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// Watches every folder of `wanted`, non-recursively, and stops watching the ones in `watched`
/// it no longer names; `watched` is kept as what is watched now.
///
/// **Answers the folders of `wanted` the platform would not watch**, which are still there:
/// a watcher that drops one and goes on as if it watched the whole set serves what it held
/// for that folder for ever (the FD-10 review). A caller that trusts its watch to tell it
/// everything treats a non-empty answer as not watching. The next call tries them again.
#[must_use = "a folder the platform would not watch is one nothing hears about"]
pub(crate) fn follow(
    watcher: &mut impl notify::Watcher,
    watched: &mut HashSet<PathBuf>,
    wanted: HashSet<PathBuf>,
) -> HashSet<PathBuf> {
    let stale: Vec<PathBuf> = watched.difference(&wanted).cloned().collect();
    for path in stale {
        // An error is a watch the platform has already dropped with its directory.
        let _ = watcher.unwatch(&path);
        watched.remove(&path);
    }
    let mut unwatched = HashSet::new();
    for path in wanted {
        if watched.contains(&path) {
            continue;
        }
        match watch_one(watcher, &path) {
            Ok(()) => {
                watched.insert(path);
            }
            // A directory that went between the listing and here is simply not watched; the
            // next batch lists again.
            Err(_) if !is_a_dir(&path) => {}
            Err(why) => {
                tracing::warn!("purlis: {} is not watched ({why})", path.display());
                unwatched.insert(path);
            }
        }
    }
    unwatched
}

/// Whether `path` is a directory itself, not a link to one.
fn is_a_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|found| found.file_type().is_dir())
}

/// One folder watched, or why not — refused here in this crate's tests when a test says so
/// ([`refuse`]).
fn watch_one(watcher: &mut impl notify::Watcher, path: &Path) -> notify::Result<()> {
    #[cfg(test)]
    if refuse::refused(path) {
        return Err(notify::Error::generic("refused by the test"));
    }
    watcher.watch(path, RecursiveMode::NonRecursive)
}

/// The platform refusing a watch, played by the tests: a folder named here is never watched,
/// whichever watcher is asked. Paths are each test's own, under a directory of its own, so
/// tests running at once do not see each other's.
#[cfg(test)]
pub(crate) mod refuse {
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, PoisonError};

    static REFUSED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    /// `path` is refused from now on.
    pub(crate) fn refuse(path: &Path) {
        REFUSED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path.to_path_buf());
    }

    /// `path` is watched again when next asked.
    pub(crate) fn allow(path: &Path) {
        REFUSED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|refused| refused != path);
    }

    pub(super) fn refused(path: &Path) -> bool {
        REFUSED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|refused| refused == path)
    }
}

/// Stops watching `path`, if it was watched: it is gone, and so is its watch (inotify drops it
/// on its own). A directory removed and made again has to be watched again, so it must not
/// still be counted as watched.
pub(crate) fn forget(
    watcher: &mut impl notify::Watcher,
    watched: &mut HashSet<PathBuf>,
    path: &Path,
) {
    if watched.remove(path) {
        let _ = watcher.unwatch(path);
    }
}

/// A watcher the tests play the platform for.
#[cfg(test)]
pub(crate) mod raw {
    use std::path::Path;

    thread_local! {
        /// What [`Raw`] was handed on this test's thread.
        static RAW: std::cell::RefCell<Option<Box<dyn notify::EventHandler>>> =
            const { std::cell::RefCell::new(None) };
    }

    /// A watcher that reports exactly the events the test sends ([`raw`], [`event`]), one at
    /// a time as inotify does: no poller's snapshot and no FSEvents latency between the test
    /// and what the watch makes of them. Made on the test's own thread.
    pub(crate) struct Raw;

    /// Every path a [`Raw`] was asked to watch, from any thread: each test's are its own,
    /// under a directory of its own.
    static WATCHES: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

    /// Paths a [`Raw`] refuses to watch, as a platform out of watches or a folder gone would.
    static REFUSED: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

    /// From now on, a [`Raw`] refuses to watch `path`.
    pub(crate) fn refuse(path: &Path) {
        REFUSED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(path.to_path_buf());
    }

    /// How many times a [`Raw`] has been asked to watch `path`.
    pub(crate) fn watched(path: &Path) -> usize {
        WATCHES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|watched| *watched == path)
            .count()
    }

    impl notify::Watcher for Raw {
        fn new<F: notify::EventHandler>(handler: F, _: notify::Config) -> notify::Result<Self> {
            RAW.with(|raw| *raw.borrow_mut() = Some(Box::new(handler)));
            Ok(Raw)
        }
        fn watch(&mut self, path: &Path, _: notify::RecursiveMode) -> notify::Result<()> {
            if REFUSED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .any(|refused| refused == path)
            {
                return Err(notify::Error::path_not_found());
            }
            WATCHES
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(path.to_path_buf());
            Ok(())
        }
        fn unwatch(&mut self, _: &Path) -> notify::Result<()> {
            Ok(())
        }
        fn kind() -> notify::WatcherKind {
            notify::WatcherKind::NullWatcher
        }
    }

    /// The platform reports `kind` on `path`.
    pub(crate) fn raw(kind: notify::EventKind, path: &Path) {
        event(notify::Event::new(kind).add_path(path.to_path_buf()));
    }

    /// The platform reports `event`.
    pub(crate) fn event(event: notify::Event) {
        RAW.with(|raw| {
            raw.borrow_mut()
                .as_mut()
                .expect("the watch has started")
                .handle_event(Ok(event));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, AccessMode, Flag, RemoveKind};
    use std::sync::mpsc;

    fn on(kind: EventKind, path: &str) -> notify::Event {
        notify::Event::new(kind).add_path(PathBuf::from(path))
    }

    #[test]
    fn a_burst_folds_every_change_queued_inside_it_into_one() {
        let (tx, rx) = mpsc::channel();
        for n in 0..10 {
            tx.send(on(
                EventKind::Create(CreateKind::File),
                &format!("/p/t{n}.md"),
            ))
            .unwrap();
        }
        let mut bursts = bursts(rx, Duration::from_millis(500), 256);

        let burst = bursts.next().expect("a burst");
        assert_eq!(burst.paths.len(), 10);
        assert!(!burst.everything);
        drop(tx);
        assert_eq!(bursts.next(), None, "a dropped watcher ends the bursts");
    }

    #[test]
    fn a_file_made_and_removed_inside_one_burst_is_still_a_change_and_a_removal() {
        let (tx, rx) = mpsc::channel();
        tx.send(on(EventKind::Create(CreateKind::File), "/p/marker"))
            .unwrap();
        tx.send(on(EventKind::Remove(RemoveKind::File), "/p/marker"))
            .unwrap();

        let burst = bursts(rx, Duration::from_millis(100), 256)
            .next()
            .expect("a burst");
        assert_eq!(burst.paths, HashSet::from([PathBuf::from("/p/marker")]));
        assert_eq!(burst.removed, HashSet::from([PathBuf::from("/p/marker")]));
    }

    #[test]
    fn a_burst_ends_on_time_under_a_flood_that_never_pauses() {
        let (tx, rx) = mpsc::channel();
        let flood = std::thread::spawn(move || {
            let mut n: u64 = 0;
            while tx
                .send(on(EventKind::Create(CreateKind::File), &format!("/p/f{n}")))
                .is_ok()
            {
                n += 1;
            }
        });
        let started = Instant::now();
        let burst = bursts(rx, Duration::from_millis(100), 256)
            .next()
            .expect("a burst");
        let took = started.elapsed();
        assert!(burst.everything, "a flood past the cap is everything");
        assert!(took < Duration::from_secs(5), "the burst took {took:?}");
        // Dropping the receiver ends the flood.
        flood.join().unwrap();
    }

    #[test]
    fn a_burst_past_its_cap_is_everything_and_holds_no_paths() {
        let mut burst = Burst::default();
        for n in 0..20 {
            burst.add(
                on(EventKind::Remove(RemoveKind::File), &format!("/p/{n}")),
                8,
            );
        }
        assert!(burst.everything);
        assert!(burst.paths.is_empty() && burst.removed.is_empty());
    }

    #[test]
    fn a_folder_made_or_anything_made_or_everything_may_have_made_a_folder() {
        let mut file = Burst::default();
        file.add(on(EventKind::Create(CreateKind::File), "/p/a"), 8);
        file.add(on(EventKind::Remove(RemoveKind::Folder), "/p/b"), 8);
        assert!(!file.made_folder);

        for kind in [CreateKind::Folder, CreateKind::Any] {
            let mut made = Burst::default();
            made.add(on(EventKind::Create(kind), "/p/new"), 8);
            assert!(made.made_folder, "{kind:?}");
        }

        let mut rescan = Burst::default();
        rescan.add(
            notify::Event::new(EventKind::Other).set_flag(Flag::Rescan),
            8,
        );
        assert!(rescan.made_folder);
    }

    #[test]
    fn a_lost_track_or_a_pathless_event_is_everything() {
        let mut rescan = Burst::default();
        rescan.add(
            notify::Event::new(EventKind::Other).set_flag(Flag::Rescan),
            8,
        );
        assert!(rescan.everything);

        let mut pathless = Burst::default();
        pathless.add(notify::Event::new(EventKind::Any), 8);
        assert!(pathless.everything);
    }

    #[test]
    fn a_read_never_reaches_a_burst() {
        let (tx, rx) = mpsc::channel();
        let mut handler = sender(tx);
        notify::EventHandler::handle_event(
            &mut handler,
            Ok(on(
                EventKind::Access(AccessKind::Open(AccessMode::Read)),
                "/p/a",
            )),
        );
        notify::EventHandler::handle_event(
            &mut handler,
            Ok(on(EventKind::Create(CreateKind::File), "/p/b")),
        );
        assert_eq!(
            rx.try_recv().expect("the change").paths,
            [PathBuf::from("/p/b")]
        );
        assert!(rx.try_recv().is_err(), "the read was sent on");
    }

    #[test]
    fn a_watcher_error_fails_closed_into_a_burst_that_is_everything() {
        let (tx, rx) = mpsc::channel();
        let mut handler = sender(tx);
        notify::EventHandler::handle_event(
            &mut handler,
            Err(notify::Error::generic("the watch was lost")),
        );
        let burst = bursts(rx, Duration::from_millis(50), 8)
            .next()
            .expect("the error was sent on");
        assert!(burst.everything);
    }

    #[test]
    fn an_idle_watch_is_said_to_be_idle_and_a_burst_still_comes_through() {
        let (tx, rx) = mpsc::channel();
        let mut bursts =
            bursts_or_idle(rx, Duration::from_millis(50), 8, Duration::from_millis(50));
        assert_eq!(bursts.next(), Some(None), "no event, so idle");
        tx.send(on(EventKind::Create(CreateKind::File), "/p/a"))
            .unwrap();
        let burst = bursts.next().expect("not ended").expect("a burst");
        assert_eq!(burst.paths, HashSet::from([PathBuf::from("/p/a")]));
        drop(tx);
        assert_eq!(bursts.next(), None, "a dropped watcher ends them");
    }
}
