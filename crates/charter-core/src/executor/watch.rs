//! What charter sees change in a plane while an extension answers (charter-app#341).
//!
//! **Detection, not a sandbox** (ADR 0041, ADR 0053 "Write scope"). An extension's program runs
//! as the operator and can write anywhere he can. What charter does is look at the plane before
//! the program starts and again after it stops, and report a change outside the paths the
//! extension declared it writes — naming the extension, beside its answer or its refusal.
//!
//! **What it looks at is what git sees**: every path `git status` lists (tracked and changed,
//! or untracked and not ignored), with its size and modification time. So it sees a file
//! written, created or deleted anywhere git would commit it, and it does not see:
//!
//! - a path git ignores — the plane's `.charter/` state among them;
//! - a write that leaves a file's size and modification time as they were;
//! - anything in a plane with no `.git` of its own at its root — one that is not a repository,
//!   or one that sits inside a larger repository — where it looks at nothing;
//! - anything at all when `git status` itself fails, which reads as a plane with nothing
//!   changed (`planegit::changed_paths` answers an empty list), so nothing is reported.
//!
//! **And it runs for every question**, a view's as much as an action's, since a view that
//! writes outside what it declared is the same overreach. In a plane where chats are writing,
//! that makes a report about a chat's write possible whenever one lands while a program answers.
//!
//! **And it cannot say who wrote.** A chat working in the same plane while the program answered
//! writes too; the report says what changed while the extension was answering, and says that
//! it is what charter saw rather than proof of who did it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How many paths a report names before it counts the rest.
const MOST_NAMED: usize = 5;

/// A path as the watch compares it: its size and modification time, or `None` when it is not
/// on disk.
type Seen = Option<(u64, Option<SystemTime>)>;

/// The plane as it was before a program started.
pub(crate) struct Before {
    plane: PathBuf,
    seen: BTreeMap<String, Seen>,
}

impl Before {
    /// Look at `plane` now, or `None` when it is not a git repository and there is nothing git
    /// would say about it.
    pub(crate) fn take(plane: &Path) -> Option<Self> {
        if !plane.join(".git").exists() {
            return None;
        }
        Some(Self {
            plane: plane.to_path_buf(),
            seen: look(plane, crate::planegit::changed_paths(plane)),
        })
    }

    /// The sentence reporting what changed since, outside `writes`, or `None` when nothing did.
    ///
    /// `may_delete` is whether the question was an action its manifest says deletes. Any other
    /// question that deleted a path is reported **even inside `writes`** — an action that
    /// deletes and did not say so skipped the question charter always asks first, and this is
    /// where that shows.
    pub(crate) fn overreach(
        self,
        extension: &str,
        writes: &[String],
        may_delete: bool,
    ) -> Option<String> {
        let now = look(&self.plane, crate::planegit::changed_paths(&self.plane));
        let every: BTreeSet<&String> = self.seen.keys().chain(now.keys()).collect();
        let mut outside = Vec::new();
        let mut deleted = Vec::new();
        for path in every {
            let after = now
                .get(path)
                .copied()
                .unwrap_or_else(|| stat(&self.plane.join(path)));
            // A path git did not list before was clean — as committed, or not there — so its
            // being listed now is the change.
            if self.seen.get(path).is_some_and(|was| *was == after) {
                continue;
            }
            if !super::extension::writes::covers(writes, path) {
                outside.push(path.as_str());
            } else if after.is_none() && !may_delete {
                deleted.push(path.as_str());
            }
        }
        if outside.is_empty() && deleted.is_empty() {
            return None;
        }
        let mut said = format!("While '{extension}' was answering, charter saw");
        if !outside.is_empty() {
            said.push_str(&format!(
                " these change outside the plane paths it declares it writes: {}",
                named(&outside)
            ));
        }
        if !deleted.is_empty() {
            if !outside.is_empty() {
                said.push_str(", and");
            }
            said.push_str(&format!(
                " these deleted though it does not say it deletes: {}",
                named(&deleted)
            ));
        }
        said.push_str(
            ". charter does not confine an extension (ADR 0041): this is what changed while it \
             answered, not proof of who changed it.",
        );
        Some(said)
    }
}

/// Every listed path with what is on disk for it.
fn look(plane: &Path, listed: Vec<String>) -> BTreeMap<String, Seen> {
    listed
        .into_iter()
        .map(|path| {
            let seen = stat(&plane.join(&path));
            (path, seen)
        })
        .collect()
}

/// A path's size and modification time, never following a link.
fn stat(at: &Path) -> Seen {
    std::fs::symlink_metadata(at)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()))
}

/// The first few paths, and how many more.
fn named(paths: &[&str]) -> String {
    let shown: Vec<String> = paths
        .iter()
        .take(MOST_NAMED)
        .map(|path| crate::shown::readable(path, 200))
        .collect();
    let more = paths.len().saturating_sub(MOST_NAMED);
    if more == 0 {
        shown.join(", ")
    } else {
        format!("{} and {more} more", shown.join(", "))
    }
}
