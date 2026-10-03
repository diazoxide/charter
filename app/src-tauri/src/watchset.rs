//! What the app's two file watchers share (`planewatch.rs`, `filewatch.rs`): which events are
//! changes, and keeping a debouncer's watches equal to a wanted set of folders, each watched
//! non-recursively.

use std::collections::HashSet;
use std::path::PathBuf;

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

/// Watches every folder of `wanted`, non-recursively, and stops watching the ones in `watched`
/// it no longer names; `watched` is kept as what is watched now.
pub(crate) fn follow<W: notify::Watcher>(
    debouncer: &mut Debouncer<W, RecommendedCache>,
    watched: &mut HashSet<PathBuf>,
    wanted: HashSet<PathBuf>,
) {
    let stale: Vec<PathBuf> = watched.difference(&wanted).cloned().collect();
    for path in stale {
        // An error is a watch the platform has already dropped with its directory.
        let _ = debouncer.unwatch(&path);
        watched.remove(&path);
    }
    for path in wanted {
        if watched.contains(&path) {
            continue;
        }
        // A directory that went between the listing and here is simply not watched; the next
        // batch lists again.
        if debouncer.watch(&path, RecursiveMode::NonRecursive).is_ok() {
            watched.insert(path);
        }
    }
}
