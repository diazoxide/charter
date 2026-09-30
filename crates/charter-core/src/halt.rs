//! The kill switch's state on disk (OV-1, ADR 0071): whether this machine's agents are
//! stopped, and the journal of every stop, re-arm and tamper.
//!
//! **Two files in charter's config directory** ([`crate::machine::dir`]), beside
//! `machine.json` and not inside it:
//!
//! - `halted`, an **empty** file whose existence says every agent is stopped. Empty because who
//!   stopped them and when is the journal's to say, and a file that says nothing but "I am here"
//!   has nothing in it to parse or to get wrong;
//! - `kill-switch.jsonl`, one [`Entry`] per line.
//!
//! **Either one says stopped** ([`stopped_on_disk`]): the marker, or a journal whose last
//! entry is a stop. So removing the marker by hand does not re-arm a machine; its journal still
//! ends on the stop. Only [`rearm`], which the app's window alone calls, writes the entry that
//! lets agents start again. `charter stop --all` stops and never re-arms.
//!
//! **The residual, stated rather than hidden.** Both files belong to the operator's user, and
//! any process running as that user — an agent among them — can edit both. A running app
//! notices a marker taken away and puts it back (the app's `killswitch` module), and journals
//! that as a [`Event::Tamper`]; it cannot stop a process that rewrites the journal as well.
//! What closes that is in ADR 0071, and is not in this module's power.

use std::io;
use std::path::{Path, PathBuf};

/// The marker's name inside [`crate::machine::dir`].
pub const FILE: &str = "halted";

/// The journal's name inside [`crate::machine::dir`].
pub const JOURNAL: &str = "kill-switch.jsonl";

/// How many lines the journal keeps, newest last. A thousand stops is years of them.
pub const JOURNAL_LINES: usize = 1000;

/// What happened to the switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// Every agent was stopped.
    Stop,
    /// The operator let agents start again, from the window.
    Rearm,
    /// The marker went away while agents were stopped, and the app put it back.
    Tamper,
}

/// Who did it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    /// The operator, in the app's window.
    Window,
    /// `charter stop --all`.
    Cli,
    /// The app itself, on noticing a tamper.
    App,
}

/// One line of the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// Epoch seconds.
    pub at: u64,
    pub event: Event,
    pub by: Actor,
}

/// A stop that could not be kept on disk. Whatever asked has still stopped what it can, and
/// has to say this loudly: the next launch, and every other process, may not know.
#[derive(Debug)]
pub struct NotKept {
    /// Why the marker could not be written.
    pub why: io::Error,
    /// Whether the journal line was written all the same, which alone keeps the stop.
    pub journaled: bool,
}

impl std::fmt::Display for NotKept {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.journaled {
            write!(
                f,
                "the stop marker could not be written ({}); the journal keeps the stop",
                self.why
            )
        } else {
            write!(
                f,
                "neither the stop marker nor the journal could be written ({}), so no other \
                 process and no later launch knows every agent was stopped",
                self.why
            )
        }
    }
}

/// Where the marker is, under `config_root`.
pub fn marker(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join(FILE)
}

/// Where the journal is, under `config_root`.
pub fn journal_path(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join(JOURNAL)
}

/// charter's directory under `config_root`, made (0700) if it is not there yet: what a watch
/// on the switch watches, which has to exist before anything is written into it.
pub fn directory(config_root: &Path) -> io::Result<PathBuf> {
    let dir = crate::machine::dir(config_root);
    crate::profiletrust::private_dir(&dir)?;
    Ok(dir)
}

/// Whether a file named `name` in charter's directory is one of the switch's own — the marker,
/// the journal, or a temporary one being renamed over either of them.
pub fn concerns(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.contains(FILE) || name.contains(JOURNAL)
}

/// Whether anything is at the marker's path, a link included: a reader that could be talked
/// out of a stop by the shape of what is there would be a switch that fails open.
pub fn marker_present(config_root: &Path) -> bool {
    marker(config_root).symlink_metadata().is_ok()
}

/// Whether the files say every agent is stopped: the marker is there, or the journal's last
/// entry is a stop that nothing re-armed.
pub fn stopped_on_disk(config_root: &Path) -> bool {
    marker_present(config_root)
        || last(config_root).is_some_and(|entry| entry.event != Event::Rearm)
}

/// Stops every agent on this machine until [`rearm`]: the marker, then one journal line.
///
/// The journal line is written even when the marker cannot be, because it alone keeps the stop
/// for [`stopped_on_disk`]. The error says which of the two was kept.
pub fn stop(config_root: &Path, by: Actor, at: u64) -> Result<(), NotKept> {
    let marked = write_marker(config_root);
    let journaled = append(
        config_root,
        Entry {
            at,
            event: Event::Stop,
            by,
        },
    )
    .is_ok();
    match marked {
        Ok(()) => Ok(()),
        Err(why) => Err(NotKept { why, journaled }),
    }
}

/// Lets agents start again. **The window's alone** (ADR 0071): nothing on the command line
/// calls it, because any agent can run a command.
///
/// The marker goes first and the journal line after. A journal line that cannot be written puts
/// the marker back and fails, so the files never say re-armed without the journal saying so.
pub fn rearm(config_root: &Path, at: u64) -> io::Result<()> {
    match std::fs::remove_file(marker(config_root)) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    let entry = Entry {
        at,
        event: Event::Rearm,
        by: Actor::Window,
    };
    if let Err(why) = append(config_root, entry) {
        let _ = write_marker(config_root);
        return Err(why);
    }
    Ok(())
}

/// Puts back a marker that went away while agents were stopped, and journals the tamper.
pub fn restore(config_root: &Path, at: u64) -> io::Result<()> {
    write_marker(config_root)?;
    append(
        config_root,
        Entry {
            at,
            event: Event::Tamper,
            by: Actor::App,
        },
    )
}

/// The journal, oldest first. A line that does not read as an [`Entry`] is left out.
pub fn journal(config_root: &Path) -> Vec<Entry> {
    std::fs::read_to_string(journal_path(config_root))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The journal's newest entry.
pub fn last(config_root: &Path) -> Option<Entry> {
    journal(config_root).pop()
}

/// Seconds since the epoch, as the journal stamps them.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn write_marker(config_root: &Path) -> io::Result<()> {
    let dir = crate::machine::dir(config_root);
    crate::profiletrust::private_dir(&dir)?;
    crate::rewrite::replace(
        &dir,
        &marker(config_root),
        b"",
        crate::rewrite::Mode::Private,
    )
}

/// Adds `entry` to the journal, keeping the newest [`JOURNAL_LINES`], the way the save journal
/// is kept (`planegit::journal_append`): read, then renamed over whole.
fn append(config_root: &Path, entry: Entry) -> io::Result<()> {
    let path = journal_path(config_root);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let line = serde_json::to_string(&entry).map_err(io::Error::other)?;
    lines.push(&line);
    let keep = &lines[lines.len().saturating_sub(JOURNAL_LINES)..];
    let mut out = keep.join("\n");
    out.push('\n');
    let dir = crate::machine::dir(config_root);
    crate::profiletrust::private_dir(&dir)?;
    crate::rewrite::replace(&dir, &path, out.as_bytes(), crate::rewrite::Mode::Private)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: u64, event: Event, by: Actor) -> Entry {
        Entry { at, event, by }
    }

    #[test]
    fn a_machine_nobody_stopped_is_not_stopped() {
        let config = tempfile::tempdir().expect("a config home");
        assert!(!stopped_on_disk(config.path()));
    }

    #[test]
    fn a_stop_is_an_empty_marker_and_one_journal_line() {
        let config = tempfile::tempdir().expect("a config home");

        stop(config.path(), Actor::Cli, 100).expect("the stop is written");

        assert!(stopped_on_disk(config.path()));
        assert_eq!(
            std::fs::read(marker(config.path())).expect("a marker"),
            b"",
            "the marker says nothing but that it is there"
        );
        assert_eq!(
            journal(config.path()),
            [entry(100, Event::Stop, Actor::Cli)]
        );
    }

    #[test]
    fn removing_the_marker_by_hand_does_not_re_arm_the_machine() {
        let config = tempfile::tempdir().expect("a config home");
        stop(config.path(), Actor::Cli, 100).expect("stopped");

        std::fs::remove_file(marker(config.path())).expect("an agent removes the marker");

        assert!(
            stopped_on_disk(config.path()),
            "the journal still ends on the stop"
        );
    }

    #[test]
    fn a_re_arm_lets_agents_start_again_and_is_journaled_as_the_window_s() {
        let config = tempfile::tempdir().expect("a config home");
        stop(config.path(), Actor::Cli, 100).expect("stopped");

        rearm(config.path(), 150).expect("re-armed");

        assert!(!stopped_on_disk(config.path()));
        assert!(!marker_present(config.path()));
        assert_eq!(
            last(config.path()),
            Some(entry(150, Event::Rearm, Actor::Window))
        );
    }

    #[test]
    fn a_restored_marker_is_journaled_as_a_tamper_and_the_machine_stays_stopped() {
        let config = tempfile::tempdir().expect("a config home");
        stop(config.path(), Actor::Window, 100).expect("stopped");
        std::fs::remove_file(marker(config.path())).expect("removed by hand");

        restore(config.path(), 120).expect("put back");

        assert!(marker_present(config.path()));
        assert!(stopped_on_disk(config.path()));
        assert_eq!(
            journal(config.path()),
            [
                entry(100, Event::Stop, Actor::Window),
                entry(120, Event::Tamper, Actor::App),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_stop_into_a_directory_made_read_only_in_advance_says_nothing_was_kept() {
        use std::os::unix::fs::PermissionsExt;
        let config = tempfile::tempdir().expect("a config home");
        let dir = crate::machine::dir(config.path());
        std::fs::create_dir_all(&dir).expect("charter's directory");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");

        let refused = stop(config.path(), Actor::Cli, 100).expect_err("nothing could be written");

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
        assert!(!refused.journaled);
        assert!(refused.to_string().contains("no later launch"), "{refused}");
    }

    #[cfg(unix)]
    #[test]
    fn a_stop_whose_marker_cannot_be_written_is_still_kept_by_its_journal_line() {
        let config = tempfile::tempdir().expect("a config home");
        // A directory where the marker goes: the rename over it fails, the journal does not.
        std::fs::create_dir_all(marker(config.path())).expect("in the marker's way");

        let refused = stop(config.path(), Actor::Cli, 100).expect_err("the marker failed");

        assert!(refused.journaled);
        assert_eq!(
            last(config.path()),
            Some(entry(100, Event::Stop, Actor::Cli))
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_marker_and_the_journal_are_the_operator_s_alone() {
        use std::os::unix::fs::PermissionsExt;
        let config = tempfile::tempdir().expect("a config home");
        stop(config.path(), Actor::Cli, 100).expect("stopped");
        for path in [marker(config.path()), journal_path(config.path())] {
            let mode = std::fs::metadata(&path)
                .expect("written")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{}", path.display());
        }
    }

    #[test]
    fn the_journal_keeps_only_the_newest_lines() {
        let config = tempfile::tempdir().expect("a config home");
        for at in 0..(JOURNAL_LINES as u64 + 5) {
            append(config.path(), entry(at, Event::Stop, Actor::Cli)).expect("appended");
        }
        let kept = journal(config.path());
        assert_eq!(kept.len(), JOURNAL_LINES);
        assert_eq!(kept[0].at, 5);
    }
}
