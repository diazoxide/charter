//! The third answer: charter asked git and git could not say. A port of the part of
//! `charter/gitstate.py` that `save` needs.
//!
//! A `git add` that failed stages nothing, so the `git diff --cached --quiet` after it says
//! there is no difference. The value that means *charter could not tell* is byte for byte the
//! value that means *there is nothing there* — and charter printed the second (charter #917:
//! `charter save` said "Nothing to save — the control-plane working tree is clean" over
//! thirteen modified files, because a zero-byte `.git/index.lock` twenty-three hours old made
//! every `add` exit 128).
//!
//! **charter reports a lock. It never clears one.** Nothing inside one command can tell a held
//! lock from an abandoned one: the holder may be a `git commit` with an editor open, or another
//! machine's view of a network mount. So charter states the three facts the person deciding
//! needs — the path, the size and the age — and leaves the `rm` to them, `ps` first.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::worktree::git::Run;

/// The name git gives the file. `$GIT_DIR/index.lock` in every git there has ever been.
pub const LOCK_NAME: &str = "index.lock";

/// How long a zero-byte lock must have sat there before charter will call it a crash rather
/// than contention. Fifteen minutes; which side it errs on matters more than the number.
pub const STALE_AFTER: f64 = 15.0 * 60.0;

/// A coarse age — `4s`, `12m`, `23h`, `3d`. Context for a human deciding whether to run `rm`,
/// never an input to a decision charter makes.
pub fn age_phrase(seconds: f64) -> String {
    // `as` from a float saturates: a negative age (a clock that moved back) and NaN are both
    // 0. So no guard for them here: one said `< 0.0` and could not be told from `<= 0.0` or
    // `== 0.0`, since the cast already answered every value it caught.
    let secs = seconds as u64;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// A `$GIT_DIR/index.lock` charter found, and the three facts about it.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexLock {
    /// The lock itself, resolved, so the sentence charter prints can be pasted into an `rm`.
    pub path: PathBuf,
    /// Bytes. Zero means git created the file and never wrote the index into it.
    pub size: u64,
    /// Seconds since its mtime.
    pub age: f64,
}

impl IndexLock {
    /// Zero bytes and older than [`STALE_AFTER`] — a crash, not contention. The conjunction
    /// is the claim: a big lock is a git that got as far as writing an index, and a young one
    /// may still be running.
    pub fn crashed(&self) -> bool {
        self.size == 0 && self.age >= STALE_AFTER
    }

    /// One line naming the lock, its size and its age — the sentence #917 needed.
    pub fn describe(&self) -> String {
        let what = if self.crashed() {
            "a git process crashed here and left it behind"
        } else {
            "something may still be holding it"
        };
        format!(
            "{} — {} byte(s), {} old; {what}.",
            self.path.display(),
            self.size,
            age_phrase(self.age)
        )
    }

    /// What the operator does next, in the order they should do it. The `rm` is second and
    /// labelled, because checking for a holder first is what makes removing the file safe.
    pub fn remedy(&self) -> [String; 2] {
        [
            "who holds it:  ps -eo pid,lstart,command | grep '[g]it'".to_string(),
            format!(
                "nobody does:   rm -f {}   (charter never removes a lock — a held one is real)",
                self.path.display()
            ),
        ]
    }
}

/// The index lock inside `git_dir`, or `None` when there is not one — and `None` too when the
/// path cannot be stat-ed at all, because turning an unreadable git directory into a
/// lock-shaped answer would be the same conflation one layer down.
pub fn find(git_dir: &Path) -> Option<IndexLock> {
    let path = git_dir.join(LOCK_NAME);
    let meta = std::fs::metadata(&path).ok()?;
    let age = meta
        .modified()
        .ok()
        .and_then(|mtime| {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
            let then = mtime.duration_since(UNIX_EPOCH).ok()?;
            Some(now.as_secs_f64() - then.as_secs_f64())
        })
        .unwrap_or(0.0);
    Some(IndexLock {
        // Resolved, because the whole point of printing it is that it can be pasted: macOS
        // hands out temp and home paths through symlinks (`/var` → `/private/var`).
        path: path.canonicalize().unwrap_or(path),
        size: meta.len(),
        age,
    })
}

/// The first line git actually wrote, or `""`.
///
/// Kept and printed rather than replaced with charter's own guess at what went wrong: "a
/// command that failed for a stated reason arrives as a command that failed" is how a full
/// disk read as a broken sweep for a day. git's stderr under a held lock is four lines of
/// which the first names the file.
pub fn said(run: &Run) -> String {
    run.err
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn lock(size: u64, age: f64) -> IndexLock {
        IndexLock {
            path: PathBuf::from("/p/.git/index.lock"),
            size,
            age,
        }
    }

    #[test]
    fn an_age_is_coarse_and_never_negative() {
        assert_eq!(age_phrase(-5.0), "0s");
        assert_eq!(age_phrase(f64::NAN), "0s");
        assert_eq!(age_phrase(4.0), "4s");
        assert_eq!(age_phrase(59.9), "59s");
        // Each unit starts exactly at its boundary, as Python's `secs < 60` has it.
        assert_eq!(age_phrase(60.0), "1m");
        assert_eq!(age_phrase(3600.0), "1h");
        assert_eq!(age_phrase(86400.0), "1d");
        assert_eq!(age_phrase(12.0 * 60.0), "12m");
        assert_eq!(age_phrase(23.0 * 3600.0), "23h");
        assert_eq!(age_phrase(3.0 * 86400.0), "3d");
    }

    #[test]
    fn only_an_empty_lock_that_has_sat_there_is_called_a_crash() {
        assert!(lock(0, STALE_AFTER).crashed());
        // A lock that got as far as holding an index is a git that was working.
        assert!(!lock(1, STALE_AFTER * 10.0).crashed());
        // And a young one may still be running, so charter does not send anyone to `rm`.
        assert!(!lock(0, STALE_AFTER - 1.0).crashed());
    }

    #[test]
    fn the_description_carries_the_path_the_size_and_the_age() {
        let said = lock(0, STALE_AFTER).describe();
        assert!(said.contains("/p/.git/index.lock"), "{said}");
        assert!(said.contains("0 byte(s)"), "{said}");
        assert!(said.contains("15m old"), "{said}");
        assert!(said.contains("crashed"), "{said}");
        assert!(lock(4, 1.0).describe().contains("may still be holding it"));
    }

    #[test]
    fn the_remedy_asks_who_holds_it_before_it_offers_the_rm() {
        let [first, second] = lock(0, STALE_AFTER).remedy();
        assert!(first.starts_with("who holds it:"), "{first}");
        assert!(second.contains("rm -f /p/.git/index.lock"), "{second}");
    }

    #[test]
    fn a_real_lock_is_found_with_its_size_and_a_directory_without_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();

        assert_eq!(find(&git_dir), None, "no lock, no answer");
        assert_eq!(
            find(&dir.path().join("nowhere")),
            None,
            "nor for a non-path"
        );

        std::fs::write(git_dir.join(LOCK_NAME), "").unwrap();
        let found = find(&git_dir).expect("the lock");
        assert_eq!(found.size, 0);
        assert!(found.age >= 0.0 && found.age < 60.0, "{}", found.age);
        assert!(found.path.ends_with("index.lock"));
    }

    #[test]
    fn git_is_quoted_rather_than_paraphrased_and_a_silent_failure_says_nothing() {
        let run = Run {
            code: Some(128),
            out: String::new(),
            err: "\n  fatal: Unable to create '/p/.git/index.lock': File exists.\nsecond\n".into(),
        };
        assert_eq!(
            said(&run),
            "fatal: Unable to create '/p/.git/index.lock': File exists."
        );
        assert_eq!(
            said(&Run {
                code: None,
                out: String::new(),
                err: "  \n\n".into()
            }),
            ""
        );
    }

    #[test]
    fn the_age_is_read_from_the_file_rather_than_assumed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK_NAME);
        std::fs::write(&path, "x").unwrap();
        let back = SystemTime::now() - Duration::from_secs(3600);
        // `set_times` is stable and needs no `unsafe`, which this workspace forbids.
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(back))
            .unwrap();

        let found = find(dir.path()).expect("the lock");
        assert_eq!(age_phrase(found.age), "1h", "{}", found.age);
        assert_eq!(found.size, 1);
    }
}
