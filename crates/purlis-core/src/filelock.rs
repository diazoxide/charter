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
//! before it closes. It is that rule for a lock taken on a plain [`File`]: the config home's
//! (`renamelocal::busy`), the event log's writer and the hook spool's. The locks taken on a
//! descriptor of their own unlock in their own guards: [`crate::rewrite::Lock`],
//! `crate::held`'s store locks, [`crate::guest`]'s, [`crate::machine`]'s, and the app's single
//! instance.

use std::fs::File;
use std::io;
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

/// How long [`lock_within`] sleeps between tries.
const A_TRY_EVERY: Duration = Duration::from_millis(5);

/// Takes `file`'s exclusive lock, trying again every few milliseconds, or answers
/// [`io::ErrorKind::TimedOut`] once `wait` has passed with another holder still on it: a wait
/// that ends, where `File::lock` waits for as long as the holder likes. `what` is what the file
/// is, as the error says it ("another process held {what} for 250 ms").
///
/// A caller that is refused this way has not got the lock, and goes on as one that could not
/// read what it guards: it never reads or writes it unlocked.
pub fn lock_within(file: File, wait: Duration, what: &str) -> io::Result<Held> {
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Held::locked(file)),
            Err(std::fs::TryLockError::WouldBlock) if started.elapsed() >= wait => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("another process held {what} for {} ms", wait.as_millis()),
                ));
            }
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(A_TRY_EVERY),
            Err(std::fs::TryLockError::Error(why)) => return Err(why),
        }
    }
}

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

    #[test]
    fn a_lock_another_holder_keeps_is_waited_for_only_so_long() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        let holder = File::create(&path).unwrap();
        holder.lock().unwrap();

        let started = Instant::now();
        let refused = lock_within(
            File::open(&path).unwrap(),
            Duration::from_millis(100),
            "the test's lock",
        )
        .expect_err("the holder keeps it");
        let waited = started.elapsed();

        assert_eq!(refused.kind(), io::ErrorKind::TimedOut, "{refused}");
        assert_eq!(
            refused.to_string(),
            "another process held the test's lock for 100 ms"
        );
        assert!(
            waited >= Duration::from_millis(100) && waited < Duration::from_secs(2),
            "waited {waited:?}"
        );

        // Let go of, it is taken, and let go of again when the taker is dropped.
        drop(holder);
        let taken = lock_within(File::open(&path).unwrap(), Duration::from_millis(100), "it")
            .expect("free now");
        drop(taken);
        File::open(&path).unwrap().try_lock().expect("free again");
    }
}
