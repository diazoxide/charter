//! The kill switch (OV-1): whether this machine's agents are stopped, and the one line each
//! stop and each re-arm leaves.
//!
//! **One file whose existence is the answer**, `halted` in charter's directory under the
//! machine's config home ([`crate::machine::dir`]), beside `machine.json` and not inside it.
//! `reporting-consent` is the precedent: a fact about the machine that one process writes and
//! another reads, where being there is the whole of what it says. It is kept out of
//! `machine.json` because that store's "five things and nothing else" is a limit, and because
//! a stop has to be written by `charter stop --all` in a terminal as readily as by the app —
//! a file that is only created and removed needs no lock and no parse to be read right.
//!
//! **Per machine, not per plane.** The switch stops every chat in every project the app holds,
//! and a plane opened after the stop must not start any either, so there is no plane it could
//! live in.
//!
//! **Outlives the app.** A stop is still in force after a quit and a relaunch: the relaunch
//! puts nothing back until the operator re-arms. That is what "nothing respawns until re-armed"
//! means for an app whose launch starts every chat its record names.
//!
//! **Re-arming is the operator's, in the window.** `charter stop --all` can stop; nothing on
//! the command line re-arms, because any agent can run a command (DECISIONS Q11: no agent's
//! message is ever consent).

use std::io;
use std::path::{Path, PathBuf};

/// The marker's name inside [`crate::machine::dir`].
pub const FILE: &str = "halted";

/// The journal's name inside [`crate::machine::dir`]: one JSON line per stop and per re-arm.
pub const JOURNAL: &str = "kill-switch.jsonl";

/// How many lines the journal keeps, newest last. A thousand stops is years of them.
pub const JOURNAL_LINES: usize = 1000;

/// Who threw the switch, for the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// The title-bar control.
    Window,
    /// `charter stop --all`.
    Cli,
}

impl By {
    fn as_str(self) -> &'static str {
        match self {
            By::Window => "window",
            By::Cli => "cli",
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

/// Whether the agents on this machine are stopped.
///
/// Anything at the marker's path counts, a link included: a reader that could be talked out
/// of a stop by the shape of what is there would be a switch that fails open.
pub fn halted(config_root: &Path) -> bool {
    marker(config_root).symlink_metadata().is_ok()
}

/// Stops every agent on this machine until [`rearm`]: writes the marker and one journal line.
///
/// The marker is written first. A journal line that cannot be written costs the record of the
/// stop, never the stop.
pub fn stop(config_root: &Path, by: By, at: u64) -> io::Result<()> {
    let dir = crate::machine::dir(config_root);
    crate::profiletrust::private_dir(&dir)?;
    let line = entry("stop", by, at);
    crate::rewrite::replace(
        &dir,
        &marker(config_root),
        format!("{line}\n").as_bytes(),
        crate::rewrite::Mode::Private,
    )?;
    append(config_root, &line);
    Ok(())
}

/// Lets agents start again. Answers whether they had been stopped; a re-arm of a machine that
/// was not stopped writes nothing.
pub fn rearm(config_root: &Path, by: By, at: u64) -> io::Result<bool> {
    match std::fs::remove_file(marker(config_root)) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    }
    append(config_root, &entry("rearm", by, at));
    Ok(true)
}

/// The journal, oldest first. A line that is not JSON is left out.
pub fn journal(config_root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(journal_path(config_root))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn entry(event: &str, by: By, at: u64) -> serde_json::Value {
    serde_json::json!({ "at": at, "event": event, "by": by.as_str() })
}

/// Adds `line` to the journal, keeping the newest [`JOURNAL_LINES`], the way the save journal
/// is kept (`planegit::journal_append`): read, then renamed over whole.
fn append(config_root: &Path, line: &serde_json::Value) {
    let path = journal_path(config_root);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let line = line.to_string();
    lines.push(&line);
    let keep = &lines[lines.len().saturating_sub(JOURNAL_LINES)..];
    let mut out = keep.join("\n");
    out.push('\n');
    let dir = crate::machine::dir(config_root);
    if crate::profiletrust::private_dir(&dir).is_ok()
        && let Err(why) =
            crate::rewrite::replace(&dir, &path, out.as_bytes(), crate::rewrite::Mode::Private)
    {
        eprintln!("charter: the kill switch's journal was not written ({why})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_nobody_stopped_is_not_halted() {
        let config = tempfile::tempdir().expect("a config home");
        assert!(!halted(config.path()));
    }

    #[test]
    fn a_stop_halts_the_machine_until_it_is_rearmed() {
        let config = tempfile::tempdir().expect("a config home");

        stop(config.path(), By::Cli, 100).expect("the stop is written");
        assert!(halted(config.path()));

        assert!(rearm(config.path(), By::Window, 200).expect("the re-arm is written"));
        assert!(!halted(config.path()));
    }

    #[test]
    fn each_stop_and_each_rearm_leaves_exactly_one_journal_line_saying_who_and_when() {
        let config = tempfile::tempdir().expect("a config home");

        stop(config.path(), By::Window, 100).expect("stopped");
        rearm(config.path(), By::Window, 150).expect("re-armed");
        stop(config.path(), By::Cli, 200).expect("stopped again");

        assert_eq!(
            journal(config.path()),
            vec![
                serde_json::json!({"at": 100, "event": "stop", "by": "window"}),
                serde_json::json!({"at": 150, "event": "rearm", "by": "window"}),
                serde_json::json!({"at": 200, "event": "stop", "by": "cli"}),
            ]
        );
    }

    #[test]
    fn rearming_a_machine_that_was_not_stopped_says_so_and_writes_nothing() {
        let config = tempfile::tempdir().expect("a config home");
        assert!(!rearm(config.path(), By::Window, 100).expect("nothing to re-arm"));
        assert!(journal(config.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn the_marker_and_the_journal_are_the_operator_s_alone() {
        use std::os::unix::fs::PermissionsExt;
        let config = tempfile::tempdir().expect("a config home");
        stop(config.path(), By::Cli, 100).expect("stopped");
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
            append(config.path(), &entry("stop", By::Cli, at));
        }
        let kept = journal(config.path());
        assert_eq!(kept.len(), JOURNAL_LINES);
        assert_eq!(kept[0]["at"], 5);
    }
}
