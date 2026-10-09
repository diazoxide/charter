//! **A folder being taken away, and the chats starting** (#1472): what keeps a chat from
//! starting in the folder of a task's branch while the person's Discard removes it.
//!
//! Discard asks whether a chat stands in the folder, and then git removes it. A chat started
//! between that look and the removal would lose its folder under it. So the two are put in one
//! order, under one lock: a start says where it is starting before it runs anything, and stays
//! said until its chat is one the app lists as open; a discard marks its folder as going before
//! it looks at the open chats, and keeps it marked until the folder is gone.
//!
//! - A start in or below a folder that is going is refused ([`Going::starting_in`]).
//! - A discard of a folder a chat is starting in or below is refused
//!   ([`Going::taking_away`]).
//! - Either way round, whichever came second sees the first: a start that came first is
//!   either still said here or already open, where the discard's look finds it.
//!
//! **In memory only**, and nothing a chat sends reaches it: both are the app's own acts.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// What [`Going`] holds, under its one lock.
#[derive(Debug, Default)]
struct Held {
    /// The folders being taken away now.
    away: Vec<(u64, PathBuf)>,
    /// The folders chats are starting in now.
    starting: Vec<(u64, PathBuf)>,
    next: u64,
}

/// The folders being taken away, and the folders chats are starting in.
#[derive(Debug, Default)]
pub(crate) struct Going(Mutex<Held>);

/// A folder marked as going, until this is dropped.
#[must_use]
pub(crate) struct GoingAway<'a> {
    going: &'a Going,
    mark: u64,
}

/// A chat starting in a folder, said until this is dropped.
#[must_use]
pub(crate) struct StartingIn<'a> {
    going: &'a Going,
    mark: u64,
}

/// `path` as the disk resolves it, so a link or a `..` on the way names the folder it reaches.
fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// What a start is told where its folder is being taken away.
pub(crate) const GOING: &str = "purlis is removing that folder: the person is discarding the \
     folder of a task's branch there, so no chat was started in it.";

/// What a discard is told where a chat is starting in the folder.
pub(crate) const STARTING: &str = "A chat is starting in that branch's folder. Close it once it \
     has started: purlis will not remove a folder a chat is working in. Nothing was removed.";

impl Going {
    fn lock(&self) -> MutexGuard<'_, Held> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Marks `folder` as going, unless a chat is starting in it or below it.
    pub(crate) fn taking_away(&self, folder: &Path) -> Result<GoingAway<'_>, String> {
        let folder = real(folder);
        let mut held = self.lock();
        if held.starting.iter().any(|(_, at)| at.starts_with(&folder)) {
            return Err(STARTING.to_owned());
        }
        held.next += 1;
        let mark = held.next;
        held.away.push((mark, folder));
        Ok(GoingAway { going: self, mark })
    }

    /// Says a chat is starting at `cwd`, unless that is in or below a folder that is going. A
    /// chat with no folder of its own starts anywhere.
    pub(crate) fn starting_in(&self, cwd: Option<&Path>) -> Result<Option<StartingIn<'_>>, String> {
        let Some(cwd) = cwd else {
            return Ok(None);
        };
        let cwd = real(cwd);
        let mut held = self.lock();
        if held.away.iter().any(|(_, folder)| cwd.starts_with(folder)) {
            return Err(GOING.to_owned());
        }
        held.next += 1;
        let mark = held.next;
        held.starting.push((mark, cwd));
        Ok(Some(StartingIn { going: self, mark }))
    }
}

impl Drop for GoingAway<'_> {
    fn drop(&mut self) {
        self.going
            .lock()
            .away
            .retain(|(mark, _)| *mark != self.mark);
    }
}

impl Drop for StartingIn<'_> {
    fn drop(&mut self) {
        self.going
            .lock()
            .starting
            .retain(|(mark, _)| *mark != self.mark);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folders() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let task = root.join("workspaces/alpha/.worktrees/api/fix-b5rc0def");
        std::fs::create_dir_all(task.join("src")).unwrap();
        let clone = root.join("workspaces/alpha/api");
        std::fs::create_dir_all(&clone).unwrap();
        (dir, task, clone)
    }

    #[test]
    fn no_chat_starts_in_a_folder_while_it_is_going_or_below_it() {
        let (_dir, task, clone) = folders();
        let going = Going::default();
        let away = going.taking_away(&task).expect("nothing starts there");

        assert_eq!(going.starting_in(Some(&task)).err().as_deref(), Some(GOING));
        assert_eq!(
            going.starting_in(Some(&task.join("src"))).err().as_deref(),
            Some(GOING)
        );
        // Through a `..` on the way, the same folder.
        assert_eq!(
            going
                .starting_in(Some(&task.join("src/..")))
                .err()
                .as_deref(),
            Some(GOING)
        );
        // Beside it, or above it, a chat starts.
        assert!(going.starting_in(Some(&clone)).is_ok());
        assert!(going.starting_in(Some(task.parent().unwrap())).is_ok());
        assert!(going.starting_in(None).is_ok());

        // Once the folder is gone, or the discard gave up, a start there is the app's to make.
        drop(away);
        assert!(going.starting_in(Some(&task)).is_ok());
    }

    #[test]
    fn a_folder_a_chat_is_starting_in_is_not_taken_away_until_the_start_is_over() {
        let (_dir, task, clone) = folders();
        let going = Going::default();
        let starting = going
            .starting_in(Some(&task.join("src")))
            .expect("nothing is going");

        assert_eq!(going.taking_away(&task).err().as_deref(), Some(STARTING));
        // A start elsewhere is no reason to keep this one.
        let elsewhere = going.starting_in(Some(&clone)).unwrap();
        drop(starting);
        assert!(going.taking_away(&task).is_ok());
        drop(elsewhere);
    }
}
