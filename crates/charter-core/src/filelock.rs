//! A file whose `flock` is let go of when it is dropped, not merely when it is closed (#1316).
//!
//! `flock` belongs to the open file description, and every copy of a descriptor shares it. A
//! program this process forks has a copy of every descriptor until it execs — close-on-exec
//! ones included, since close-on-exec only acts AT the exec — and a program started with a
//! descriptor handed to it keeps that copy for its life. Closing the holder's descriptor alone
//! therefore leaves the lock held for as long as any copy lives: a lock dropped while another
//! thread is spawning stays held until that child execs, which under load is long enough for
//! the next taker to be refused.
//!
//! `flock(LOCK_UN)` on any one copy lets go of the lock for all of them, so [`Held`] unlocks
//! before it closes. Every lock charter takes for longer than a call ([`crate::rewrite::Lock`],
//! [`crate::held`], [`crate::guest`], [`crate::machine`]'s, the app's single instance) already
//! unlocks the same way; this is that rule for the locks taken on a plain [`File`].

use std::fs::File;
use std::ops::{Deref, DerefMut};

/// A [`File`] this process holds an `flock` on, unlocked when it is dropped.
#[derive(Debug)]
pub struct Held(File);

impl Held {
    /// `file`, which this process has just locked.
    pub fn locked(file: File) -> Self {
        Self(file)
    }
}

impl Deref for Held {
    type Target = File;

    fn deref(&self) -> &File {
        &self.0
    }
}

impl DerefMut for Held {
    fn deref_mut(&mut self) -> &mut File {
        &mut self.0
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for Held {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        // An unlock that fails leaves only what a close leaves: the lock goes with the last
        // copy of the descriptor, which is no worse than before.
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_is_free_once_dropped_though_a_copy_of_its_descriptor_lives_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        let file = File::create(&path).unwrap();
        file.lock().unwrap();
        let held = Held::locked(file);
        let a_forked_child_s_copy = held.try_clone().unwrap();

        drop(held);

        File::open(&path)
            .unwrap()
            .try_lock()
            .expect("the lock is free once its holder is dropped");
        drop(a_forked_child_s_copy);
    }
}
