//! Temp files an older charter left behind when it was killed mid-write (#440).
//!
//! Before #432 and #434 each whole-file writer named its own temp file beside its target:
//! `reopen.json.writing`, `machine.json.<pid>.<tag>.writing` and so on. Every writer now goes
//! through [`crate::rewrite::replace`], whose temps are `.charter-generated.*.tmp`, so nothing
//! that runs today ever writes the old names again — and a temp an older charter was killed
//! in front of is never renamed away or cleaned up by anyone.
//!
//! **Swept when the app opens a plane**, which is when every writer that used the old names
//! (the reopen record, the launched-profiles record, the machine store, the extension record,
//! the window layout) is about to run again. Only a name that is exactly one of the old
//! shapes, a plain file (never a link), and older than [`STALE_AFTER`] is removed: a name
//! charter did not make is not charter's to remove, and a temp an older charter still running
//! beside this one is writing right now is younger than that.
//!
//! The gate on the way to the directory is a `stat`, and the removal is by path, so a
//! directory swapped for a link in between is ADR 0028's shared window; what could be lost
//! there is a file with exactly one of these names, older than the threshold.

use std::path::Path;
use std::time::{Duration, SystemTime};

/// How old a leftover must be before it is removed. A whole-file write takes milliseconds;
/// this is long enough that an older charter still running beside this one is never cut off
/// mid-write.
pub const STALE_AFTER: Duration = Duration::from_secs(10 * 60);

/// The old names that carried a pid and a per-call tag: `<stem>.<pid>.<tag>.<ending>`.
const TAGGED: [(&str, &str); 4] = [
    ("machine.json", "writing"),
    ("extensions.json", "writing"),
    ("layout.json", "writing"),
    ("harness-profiles-launched.json", "tmp"),
];

/// The one old name that carried neither: the reopen record's.
const FIXED: &str = "reopen.json.writing";

/// Whether `name` is exactly a temp file name an older charter used.
pub fn is_old_temp(name: &str) -> bool {
    if name == FIXED {
        return true;
    }
    TAGGED.iter().any(|(stem, ending)| {
        let Some(middle) = name
            .strip_prefix(stem)
            .and_then(|rest| rest.strip_prefix('.'))
            .and_then(|rest| rest.strip_suffix(ending))
            .and_then(|rest| rest.strip_suffix('.'))
        else {
            return false;
        };
        let Some((pid, tag)) = middle.split_once('.') else {
            return false;
        };
        // The pid in decimal, and `workspaces::scratch_tag`'s twelve lowercase hex digits.
        !pid.is_empty()
            && pid.bytes().all(|b| b.is_ascii_digit())
            && tag.len() == 12
            && tag.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// Remove the leftovers in `dir` older than [`STALE_AFTER`] at `now`; how many went.
///
/// `dir` is reached from `root` with no link on the way, or nothing is swept. A leftover is
/// asked about without following a link (`DirEntry::file_type` is the entry's own), and a
/// link is left alone: it is not a temp charter made.
pub fn sweep(root: &Path, dir: &Path, now: SystemTime) -> usize {
    if crate::contain::no_link_on_the_way(root, dir).is_err() {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        if !is_old_temp(&entry.file_name().to_string_lossy()) {
            continue;
        }
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|found| found.modified())
            .is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age >= STALE_AFTER));
        if old_enough && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// The plane's leftovers: `.charter/` (the launched-profiles record's) and `.charter/app/`
/// (the reopen record's).
pub fn sweep_plane(root: &Path) -> usize {
    let state = root.join(".charter");
    let now = SystemTime::now();
    sweep(root, &state, now) + sweep(root, &state.join("app"), now)
}

/// The machine's leftovers, in the config home's charter directory: the store's, the
/// extension record's and the window layout's.
pub fn sweep_config(config_root: &Path) -> usize {
    sweep(
        config_root,
        &crate::machine::dir(config_root),
        SystemTime::now(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file at `path`, last modified `age` ago.
    fn left(path: &Path, age: Duration) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"half a record").unwrap();
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }

    const OLD: Duration = Duration::from_secs(60 * 60);

    #[test]
    fn the_names_an_older_charter_used_are_known_and_nothing_else_is() {
        for name in [
            "reopen.json.writing",
            "machine.json.4242.0123456789ab.writing",
            "extensions.json.1.abcdefabcdef.writing",
            "layout.json.77.000000000000.writing",
            "harness-profiles-launched.json.9.0a0b0c0d0e0f.tmp",
        ] {
            assert!(is_old_temp(name), "{name}");
        }
        for name in [
            "reopen.json",
            "notes.writing",
            "reopen.json.writing.bak",
            "machine.json.writing",
            "machine.json.4242.writing",
            "machine.json.x42.0123456789ab.writing",
            "machine.json.4242.0123456789AB.writing",
            "machine.json.4242.0123456789a.writing",
            "machine.json.4242.0123456789ab.tmp",
            "layout.json.77.000000000000.writing.x",
            "harness-profiles-launched.json.9.0a0b0c0d0e0f.writing",
            ".charter-generated.reopen.json.1.0123456789ab.tmp",
        ] {
            assert!(!is_old_temp(name), "{name}");
        }
    }

    #[test]
    fn a_planes_sweep_removes_the_temps_an_older_charter_left_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let reopen = root.join(".charter/app/reopen.json.writing");
        let launched = root.join(".charter/harness-profiles-launched.json.12.0123456789ab.tmp");
        let record = root.join(".charter/app/reopen.json");
        let theirs = root.join(".charter/app/notes.writing");
        let fresh = root.join(".charter/harness-profiles-launched.json.13.0123456789ab.tmp");
        left(&reopen, OLD);
        left(&launched, OLD);
        left(&record, OLD);
        left(&theirs, OLD);
        left(&fresh, Duration::ZERO);

        assert_eq!(sweep_plane(root), 2);

        assert!(!reopen.exists() && !launched.exists());
        assert!(record.exists() && theirs.exists() && fresh.exists());
    }

    #[test]
    fn a_temp_younger_than_the_threshold_is_left_for_the_charter_writing_it() {
        let dir = tempfile::tempdir().unwrap();
        let reopen = dir.path().join(".charter/app/reopen.json.writing");
        left(&reopen, Duration::from_secs(5));
        assert_eq!(sweep_plane(dir.path()), 0);
        assert!(reopen.exists());
    }

    #[test]
    fn the_config_homes_leftovers_are_swept_too() {
        let dir = tempfile::tempdir().unwrap();
        let home = crate::machine::dir(dir.path());
        let names = [
            "machine.json.4242.0123456789ab.writing",
            "extensions.json.4242.0123456789ab.writing",
            "layout.json.4242.0123456789ab.writing",
        ];
        for name in names {
            left(&home.join(name), OLD);
        }
        left(&home.join("machine.json"), OLD);

        assert_eq!(sweep_config(dir.path()), 3);

        assert!(names.iter().all(|name| !home.join(name).exists()));
        assert!(home.join("machine.json").exists());
    }

    /// A leftover's name that is a link is left alone, and so is what it points at; a
    /// `.charter/` that is a link is not swept at all.
    #[cfg(unix)]
    #[test]
    fn a_link_is_never_swept_and_never_swept_through() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("plane");
        let elsewhere = tempfile::tempdir().unwrap();
        let theirs = elsewhere.path().join("theirs");
        left(&theirs, OLD);
        let at = root.join(".charter/app/reopen.json.writing");
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&theirs, &at).unwrap();

        assert_eq!(sweep_plane(&root), 0);
        assert!(at.is_symlink() && theirs.exists());

        std::fs::remove_dir_all(root.join(".charter")).unwrap();
        left(&elsewhere.path().join("app/reopen.json.writing"), OLD);
        std::os::unix::fs::symlink(elsewhere.path(), root.join(".charter")).unwrap();
        assert_eq!(sweep_plane(&root), 0);
        assert!(elsewhere.path().join("app/reopen.json.writing").exists());
    }
}
