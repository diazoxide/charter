//! When the app saves a plane by itself (charter-app#296, ADR 0051).
//!
//! **A decision, not a loop.** [`Quiet::tick`] is told what the plane looks like now and what
//! time it is, and answers whether to save. The loop that asks it, the clock and the save are
//! the app's; everything that decides is here, where a test can drive it with a made-up clock.

use std::time::{Duration, Instant};

use crate::planegit::{Stage, Standing};
use crate::planesave::{Mode, Plane};

/// How long a save that did not settle the plane — a push to a remote that is down — waits
/// before auto-save tries again, however short the quiet period is.
pub const RETRY_AFTER: Duration = Duration::from_secs(300);

/// What auto-save decides on one look at a plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Wait,
    Save,
}

/// Whether auto-save is on for a plane: `[plane] autosave`, and a mode that saves. A plane that
/// names no mode has not been asked yet, and is never saved by itself before it has.
pub fn on(plane: &Plane) -> bool {
    plane.autosave.value && matches!(plane.mode.value, Some(mode) if mode != Mode::Off)
}

/// Whether a save of a plane standing like this would do anything: files to commit, or commits
/// a push would carry — to the target branch, or in a PR mode to the save branch and its pull
/// request (`standing` counts those as committed until the pull request carries them).
pub fn worth_saving(standing: &Standing) -> bool {
    !standing.changed.is_empty() || (standing.stage == Stage::Committed && standing.pushes)
}

/// One plane's quiet period: what it last looked like, since when, and whether a save was
/// already tried on that look.
#[derive(Debug, Default)]
pub struct Quiet {
    seen: Option<Seen>,
}

#[derive(Debug)]
struct Seen {
    fingerprint: String,
    since: Instant,
    tried: bool,
}

impl Quiet {
    /// Look at the plane at `now`. `fingerprint` changes whenever what is unsaved changes
    /// ([`crate::planegit::fingerprint`]); `quiet` is `[plane] autosave_after`.
    pub fn tick(
        &mut self,
        now: Instant,
        plane: &Plane,
        standing: &Standing,
        fingerprint: &str,
    ) -> Decision {
        // Off, paused while blocked, or nothing to take: forget the quiet period, so the next
        // change starts a fresh one.
        if !on(plane) || standing.stage == Stage::Blocked || !worth_saving(standing) {
            self.seen = None;
            return Decision::Wait;
        }
        let quiet = plane.autosave_after.value;
        match &mut self.seen {
            Some(seen) if seen.fingerprint == fingerprint => {
                let wait = if seen.tried {
                    quiet.max(RETRY_AFTER)
                } else {
                    quiet
                };
                if now.saturating_duration_since(seen.since) < wait {
                    return Decision::Wait;
                }
                seen.since = now;
                seen.tried = true;
                Decision::Save
            }
            _ => {
                self.seen = Some(Seen {
                    fingerprint: fingerprint.to_owned(),
                    since: now,
                    tried: false,
                });
                Decision::Wait
            }
        }
    }
}

/// What saving a plane as the app quits came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtQuit {
    /// Auto-save is off, or there was nothing to save.
    Nothing,
    /// Committed; the save stops at the commit, or there is nothing to push to.
    Committed,
    /// Committed and pushed within the bound.
    Pushed,
    /// Committed, and the push had not finished within the bound. The launch after this one
    /// pushes what is left ([`on`] and [`worth_saving`] still say so then).
    CommittedPushStillRunning,
    /// The commit itself was refused — a secret, a signer — and is in the journal.
    Refused,
}

/// Save the plane at `root` as the app quits (ADR 0051): commit at once — a commit is local and
/// quick, and it is what makes the work safe on this machine — then give the push no more than
/// `bound`. A push still running then is left to finish or fail on its own; the app does not
/// wait for a remote that never answers.
pub fn at_quit(root: &std::path::Path, bound: Duration) -> AtQuit {
    let plane = crate::planesave::Settings::read(root).plane;
    let standing = crate::planegit::standing(root);
    if !on(&plane) || standing.stage == Stage::Blocked || !worth_saving(&standing) {
        return AtQuit::Nothing;
    }
    // Whatever else is saving this plane — the worker's last save — finishes first, within the
    // bound; the commit and the push below then hold the plane, as every save does.
    let Some(claim) = crate::planegit::Claim::within(root, bound) else {
        return AtQuit::Nothing;
    };
    if !standing.changed.is_empty() {
        let code = crate::planegit::save_claimed(
            &crate::planegit::Request {
                root,
                message: None,
                sign: false,
                no_push: true,
                cwd: root,
            },
            crate::planegit::Trigger::Quit,
            &claim,
            &mut |_| {},
        );
        if code != 0 {
            return AtQuit::Refused;
        }
    }
    if !crate::planegit::standing(root).pushes {
        return AtQuit::Committed;
    }
    let (told, heard) = std::sync::mpsc::channel();
    let at = root.to_path_buf();
    let sign = plane.sign.value;
    std::thread::spawn(move || {
        // The claim goes with the push, and is let go of when the push ends.
        let _claim = claim;
        // As the mode says: the target branch, or the save branch and its pull request.
        let pushed = crate::planegit::push_saved(&at, sign, &mut |_| {});
        let _ = told.send(matches!(
            pushed.outcome,
            crate::planegit::Outcome::Pushed | crate::planegit::Outcome::PrOpen
        ));
    });
    match heard.recv_timeout(bound) {
        Ok(true) => AtQuit::Pushed,
        Ok(false) => AtQuit::Committed,
        Err(_) => AtQuit::CommittedPushStillRunning,
    }
}

#[cfg(test)]
mod tests;
