//! What the app's two file watchers share (`planewatch.rs`, `filewatch.rs`): which events are
//! changes, and keeping a watcher's watches equal to a wanted set of folders, each watched
//! non-recursively.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use notify::RecursiveMode;
use notify::event::{EventKind, MetadataKind, ModifyKind};
use notify_debouncer_full::{Debouncer, RecommendedCache};

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

/// What watches folders one at a time: a debouncer, or a platform watcher whose events are
/// folded by hand ([`Plain`]).
pub(crate) trait Folders {
    fn watch(&mut self, path: &Path) -> notify::Result<()>;
    fn unwatch(&mut self, path: &Path) -> notify::Result<()>;
}

impl<W: notify::Watcher> Folders for Debouncer<W, RecommendedCache> {
    fn watch(&mut self, path: &Path) -> notify::Result<()> {
        Debouncer::watch(self, path, RecursiveMode::NonRecursive)
    }
    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        Debouncer::unwatch(self, path)
    }
}

/// A platform watcher on its own, with no debouncer between it and the caller.
pub(crate) struct Plain<W>(pub W);

impl<W: notify::Watcher> Folders for Plain<W> {
    fn watch(&mut self, path: &Path) -> notify::Result<()> {
        self.0.watch(path, RecursiveMode::NonRecursive)
    }
    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        self.0.unwatch(path)
    }
}

/// Watches every folder of `wanted`, non-recursively, and stops watching the ones in `watched`
/// it no longer names; `watched` is kept as what is watched now.
pub(crate) fn follow(
    folders: &mut impl Folders,
    watched: &mut HashSet<PathBuf>,
    wanted: HashSet<PathBuf>,
) {
    let stale: Vec<PathBuf> = watched.difference(&wanted).cloned().collect();
    for path in stale {
        // An error is a watch the platform has already dropped with its directory.
        let _ = folders.unwatch(&path);
        watched.remove(&path);
    }
    for path in wanted {
        if watched.contains(&path) {
            continue;
        }
        // A directory that went between the listing and here is simply not watched; the next
        // batch lists again.
        if folders.watch(&path).is_ok() {
            watched.insert(path);
        }
    }
}
