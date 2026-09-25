//! The alerts drawer's one question: what is wrong in every project this process holds.
//!
//! **Cross-project by construction.** An alert is about a plane — its pin, its front door,
//! its workspaces' layout, its root — and not about the workspace or the chat on screen, so
//! the drawer is the window's and asks about every plane at once. The command takes no plane
//! for exactly that reason: a drawer that could be asked about one project could be wired to
//! the one in front and quietly stop being about the rest.
//!
//! The deciding is `charter_core::alerts`, the port of charter's `_alerts`, which also draws
//! the terminal status line's rows — so the drawer and `charter statusline` cannot disagree
//! about whether a plane has anything to say.

use std::path::Path;

use charter_core::alerts;

use crate::planes::PlaneId;

/// One alert, as the drawer draws it.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct AlertRow {
    /// `warn` or `bad` — charter's two accents above plain text. `bad` is the one that loses
    /// work if it is left: a nested plane, a memory commit that reached no remote.
    severity: String,
    /// What it is about, in charter's word for it: `charter`, `front door`, `reinit`,
    /// `nested plane`, `plane root`.
    subject: String,
    /// What is wrong.
    detail: String,
    /// The command, or the step, that fixes it.
    remedy: String,
}

/// One open project's alerts.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct PlaneAlerts {
    plane: PlaneId,
    alerts: Vec<AlertRow>,
    /// Why charter stopped looking before the last alert, or null when it looked at every one.
    /// **The alerts above stand, and their number is not this project's number of alerts** —
    /// a count drawn from a reading that stopped would be smaller than the truth with nothing
    /// on it saying so.
    stopped: Option<String>,
}

/// One project's reading. The app flags no workspace's layout of its own, so every stale
/// workspace counts; and it stands in the plane it opened, so a plane nested inside another's
/// `workspaces/` is said to be.
pub(crate) fn of(plane: PlaneId, root: &Path) -> PlaneAlerts {
    let (alerts, stopped) = rows(root);
    PlaneAlerts {
        plane,
        alerts,
        stopped,
    }
}

/// The core's alert as the drawer shows it beside the title bar's save indicator
/// (charter-app#332): a plane root's uncommitted and unpushed findings are what the indicator
/// already says, so the drawer keeps only what it does not — a detached HEAD, a branch that is
/// not the default — and drops the row when nothing is left. The terminal status line, which has
/// no indicator, still gets the whole row from the core.
fn beside_the_indicator(alert: &alerts::Alert) -> Option<alerts::Alert> {
    match alert {
        alerts::Alert::PlaneRoot {
            name,
            detached,
            off,
            ..
        } => (*detached || off.is_some()).then(|| alerts::Alert::PlaneRoot {
            name: name.clone(),
            dirty: false,
            detached: *detached,
            off: off.clone(),
            memory: None,
        }),
        other => Some(other.clone()),
    }
}

/// How long a save may stay blocked before the drawer says so (ADR 0051). A secret the scan
/// caught is said at once: that save never goes through without somebody.
const BLOCKED_FOR: f64 = 600.0;

/// The drawer's row for a plane whose save is blocked, once it has been for [`BLOCKED_FOR`] —
/// or at once for a secret. The app's alone, not the core's: the terminal status line reads the
/// core's alerts, and the Saving view is where this one is resolved.
fn save_blocked(root: &Path, now: f64) -> Option<AlertRow> {
    use charter_core::planegit;
    let standing = planegit::standing(root);
    let why = standing.blocked?;
    // A secret is said at once: that save never goes through without somebody.
    let secret = why == planegit::SECRET_REFUSED;
    // Otherwise from when the cause began, read from the cause itself: the push record for a
    // conflict or a stranded push, the journal line that recorded a branch mismatch. No cause
    // with a time, no alert — never a clock that restarts on every look.
    let since = if !standing.conflicts.is_empty() || planegit::unlanded(root).is_some() {
        planegit::push_record(root).and_then(|r| r["at"].as_f64())
    } else {
        planegit::journal(root)
            .last()
            .filter(|line| line["outcome"] == "blocked")
            .and_then(|line| line["at"].as_f64())
    };
    if !secret && since.is_none_or(|since| now - since < BLOCKED_FOR) {
        return None;
    }
    Some(AlertRow {
        severity: "bad".to_owned(),
        subject: "save".to_owned(),
        detail: format!("the plane's save is blocked: {why}"),
        remedy: "Open the Saving tab: resolve it in a chat, or in a terminal".to_owned(),
    })
}

/// Seconds since the epoch, as the journal writes them.
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// The rows, and why the reading stopped where it did.
fn rows(root: &Path) -> (Vec<AlertRow>, Option<String>) {
    let reading = alerts::read(&alerts::Asking {
        root,
        active: None,
        standing: root,
    });
    (
        reading
            .alerts
            .iter()
            .filter_map(beside_the_indicator)
            .map(|alert| {
                let shown = alert.shown();
                AlertRow {
                    severity: match shown.severity {
                        alerts::Severity::Warn => "warn",
                        alerts::Severity::Bad => "bad",
                    }
                    .to_owned(),
                    subject: shown.subject,
                    detail: shown.detail,
                    remedy: shown.remedy,
                }
            })
            .chain(save_blocked(root, now()))
            .collect(),
        reading.stopped,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawer_gets_charters_words_for_each_alert() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(
            root.join("charter.toml"),
            "schema = 1\n\n[persona]\ndefault = \"ghost\"\n",
        )
        .unwrap();
        let (alerts, stopped) = rows(&root);
        assert_eq!(stopped, None);
        assert_eq!(alerts.len(), 1, "{alerts:?}");
        assert_eq!(alerts[0].severity, "warn");
        assert_eq!(alerts[0].subject, "front door");
        assert_eq!(alerts[0].detail, "ghost — no such persona");
        assert_eq!(alerts[0].remedy, "charter persona default <name>");
    }

    #[test]
    fn a_reading_that_stopped_says_why_and_keeps_what_it_found() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n[[[ not toml\n").unwrap();
        let (alerts, stopped) = rows(&root);
        assert!(stopped.is_some(), "{alerts:?}");
        assert!(alerts.is_empty());
    }

    #[test]
    fn the_drawer_leaves_to_the_save_indicator_what_it_already_says_about_the_plane_root() {
        let dirty = alerts::Alert::PlaneRoot {
            name: "plane".into(),
            dirty: true,
            detached: false,
            off: None,
            memory: Some(alerts::Memory::NotPushed),
        };
        assert_eq!(beside_the_indicator(&dirty), None);

        let off = alerts::Alert::PlaneRoot {
            name: "plane".into(),
            dirty: true,
            detached: false,
            off: Some(("work".into(), "main".into())),
            memory: Some(alerts::Memory::NotPushed),
        };
        assert_eq!(
            beside_the_indicator(&off),
            Some(alerts::Alert::PlaneRoot {
                name: "plane".into(),
                dirty: false,
                detached: false,
                off: Some(("work".into(), "main".into())),
                memory: None,
            })
        );
    }

    fn blocked_plane(detail: &str, at: f64) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("charter.toml"), "[plane]\nmode = \"push\"\n").unwrap();
        for args in [
            &["init", "-q"][..],
            &["add", "-A"],
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "one"],
        ] {
            let mut command = std::process::Command::new("git");
            command
                .arg("-C")
                .arg(root)
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.invalid");
            charter_core::forklock::output(&mut command).expect("git runs");
        }
        let journal = charter_core::planegit::journal_path(root);
        std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
        std::fs::write(
            journal,
            format!(
                "{}\n",
                serde_json::json!({"at": at, "outcome": "blocked", "detail": detail})
            ),
        )
        .unwrap();
        dir
    }

    #[test]
    fn a_save_blocked_by_a_secret_is_said_at_once() {
        let dir = blocked_plane("a secret-shaped value in a memory or ref file", now());
        let memory = dir.path().join("personas/steward/memory");
        std::fs::create_dir_all(&memory).unwrap();
        std::fs::write(
            memory.join("m.md"),
            "token: ghp_0123456789abcdefghijklmnopqrstuvwxyz\n",
        )
        .unwrap();
        let row = save_blocked(dir.path(), now()).expect("said at once");
        assert_eq!(
            (row.severity.as_str(), row.subject.as_str()),
            ("bad", "save")
        );
        assert!(row.detail.contains("secret-shaped"), "{row:?}");
    }

    #[test]
    fn any_other_block_is_said_after_ten_minutes_and_not_before() {
        let at = now();
        // The plane on another branch than it saves into, recorded by the save that stopped.
        let dir = blocked_plane("this plane is on main, and [plane] branch is trunk", at);
        std::fs::write(
            dir.path().join("charter.toml"),
            "[plane]\nmode = \"push\"\nbranch = \"trunk\"\n",
        )
        .unwrap();
        assert!(save_blocked(dir.path(), at + 60.0).is_none());
        assert!(save_blocked(dir.path(), at + 601.0).is_some());
    }
}
