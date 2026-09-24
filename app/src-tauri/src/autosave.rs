//! Auto-save, in the app (charter-app#296, ADR 0051): one worker per held plane, beside its
//! file watcher, started when the plane is held and stopped when it is let go of.
//!
//! There is no daemon (ADR 0025): a plane is saved by itself only while the app holds it. What
//! decides is the core's ([`charter_core::autosave`]); this module is the clock and the loop.
//!
//! On each look the worker:
//! - saves once the plane has been quiet for `[plane] autosave_after` ([`autosave::Quiet`]);
//! - saves at once after a chat in the plane ends ([`Poke::SessionEnded`]);
//! - on its first look, pushes what the last run left committed and unpushed (the launch);
//! - every [`FETCH_EVERY`], and when the window asks ([`Poke::Fetch`]), fetches the target
//!   branch — and fast-forwards a clean tree onto it only while auto-save is on;
//! - and after any of these that did something, tells the window the plane changed.
//!
//! Saving at quit is not the worker's: the app is exiting, so [`at_quit`] runs it bounded,
//! for every held plane at once.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use charter_core::autosave::{self, Decision, Quiet};
use charter_core::planegit::{self, Stage, Trigger};
use charter_core::planesave;

use crate::planes::PlaneId;

/// How often the worker looks at its plane: one `git status`.
const LOOK_EVERY: Duration = Duration::from_secs(2);
/// How often it fetches the target branch while nothing asks sooner.
pub const FETCH_EVERY: Duration = Duration::from_secs(300);
/// The least time between two fetches, however often the window asks.
const FETCH_AT_MOST_EVERY: Duration = Duration::from_secs(60);
/// How long quitting gives the pushes.
pub const QUIT_BOUND: Duration = Duration::from_secs(5);

/// What the worker can be told between looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Poke {
    /// A chat in the plane ended: save now, if there is anything to save.
    SessionEnded,
    /// The window came back into focus: fetch, unless the last fetch was a moment ago.
    Fetch,
}

/// A plane's auto-save worker. Dropping it stops the loop at its next look: the flag is
/// what stops it, because a chat's end still holds a sender, and a channel alone would keep a
/// let-go plane's worker looking until the last of those was gone.
pub struct Worker {
    poke: Sender<Poke>,
    stop: Arc<AtomicBool>,
    /// Never joined: a worker mid-save is left to finish rather than holding a plane's close
    /// open. Kept so a test can see the loop end.
    #[cfg_attr(not(test), expect(dead_code, reason = "read only by the stop test"))]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake it now rather than at its next look; a worker mid-save finishes that save.
        let _ = self.poke.send(Poke::Fetch);
    }
}

impl Worker {
    /// Start looking at the plane at `root`. `changed` tells the window it moved.
    pub fn start(plane: PlaneId, root: PathBuf, changed: crate::planewatch::Changed) -> Self {
        let (poke, poked) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let spawned = std::thread::Builder::new()
            .name("charter-autosave".into())
            .spawn(move || run(&root, &poked, &stopping, &|| changed(plane.clone())));
        let thread = spawned
            .map_err(|why| eprintln!("charter: auto-save did not start ({why}); save by hand"))
            .ok();
        Self { poke, stop, thread }
    }

    /// Something for the worker to act on at its next look.
    pub fn poker(&self) -> Sender<Poke> {
        self.poke.clone()
    }
}

/// The loop: a look every [`LOOK_EVERY`] or at a poke, until the worker is dropped.
fn run(root: &Path, poked: &Receiver<Poke>, stop: &AtomicBool, tell: &dyn Fn()) {
    let mut state = State::default();
    let mut poke = None;
    loop {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        if state.look(root, Instant::now(), poke) {
            tell();
        }
        poke = match poked.recv_timeout(LOOK_EVERY) {
            Ok(poke) => Some(poke),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
    }
}

/// What the worker remembers between looks.
#[derive(Debug, Default)]
struct State {
    quiet: Quiet,
    launched: bool,
    fetched: Option<Instant>,
}

impl State {
    /// One look at the plane at `root`. Answers whether anything was done the window should
    /// hear about.
    fn look(&mut self, root: &Path, now: Instant, poke: Option<Poke>) -> bool {
        let plane = planesave::Settings::read(root).plane;
        let mut did = false;

        let due = match self.fetched {
            None => true,
            Some(at) if poke == Some(Poke::Fetch) => now.duration_since(at) >= FETCH_AT_MOST_EVERY,
            Some(at) => now.duration_since(at) >= FETCH_EVERY,
        };
        if due {
            self.fetched = Some(now);
            if let Ok(incoming) = planegit::fetch(root, autosave::on(&plane)) {
                did |= incoming.behind > 0;
            }
        }
        // Off: what came in is still fetched and shown, and nothing else costs a git process.
        if !autosave::on(&plane) {
            return did;
        }

        let standing = planegit::standing(root);
        let trigger = if !self.launched {
            self.launched = true;
            // What the last run committed and could not push, pushed first.
            (autosave::on(&plane) && standing.stage == Stage::Committed && standing.pushes)
                .then_some(Trigger::Launch)
        } else if poke == Some(Poke::SessionEnded) {
            (autosave::on(&plane)
                && standing.stage != Stage::Blocked
                && autosave::worth_saving(&standing))
            .then_some(Trigger::SessionEnd)
        } else {
            None
        };
        let trigger = trigger.or_else(|| {
            let seen = planegit::fingerprint_of(root, &standing);
            (self.quiet.tick(now, &plane, &standing, &seen) == Decision::Save)
                .then_some(Trigger::Quiet)
        });
        if let Some(trigger) = trigger {
            // A refusal is in the journal, in its own words; the stage says blocked.
            let _ = crate::saving::save_as(root, None, trigger);
            did = true;
        }
        did
    }
}

/// The window came back into focus: fetch the plane's target branch, unless it was fetched a
/// moment ago. Answers at once; what the fetch finds reaches the window as a plane change.
#[tauri::command]
#[specta::specta]
pub fn plane_fetch(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: PlaneId,
) -> Result<(), String> {
    planes.held(&plane)?.poke_autosave(Poke::Fetch);
    Ok(())
}

/// Save every plane in `roots` as the app quits, all at once (ADR 0051), and return within
/// [`QUIT_BOUND`] and a moment more whatever happens: a commit that waits on a signer or a
/// slow hook is left to finish on its own rather than holding the app open.
pub fn at_quit(roots: Vec<PathBuf>) {
    let (done, finished) = mpsc::channel();
    let started = roots.len();
    for root in roots {
        let done = done.clone();
        std::thread::spawn(move || {
            autosave::at_quit(&root, QUIT_BOUND);
            let _ = done.send(());
        });
    }
    let until = Instant::now() + QUIT_BOUND + Duration::from_secs(1);
    for _ in 0..started {
        let left = until.saturating_duration_since(Instant::now());
        if finished.recv_timeout(left).is_err() {
            return;
        }
    }
}

#[cfg(test)]
impl Worker {
    /// Drop it and wait up to `bound` for its thread to end: whether it did.
    fn stop_within(mut self, bound: Duration) -> bool {
        let thread = self.thread.take();
        drop(self);
        let until = Instant::now() + bound;
        let Some(thread) = thread else { return true };
        while Instant::now() < until {
            if thread.is_finished() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(dir)
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid");
        let out = charter_core::forklock::output(&mut command).expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A plane with one commit, its own identity, and no remote.
    fn plane(toml: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("charter.toml"), toml).unwrap();
        std::fs::write(root.join(".gitignore"), ".charter/\n").unwrap();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "user.name", "t"]);
        git(root, &["config", "user.email", "t@example.invalid"]);
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "one"]);
        dir
    }

    fn commits(root: &Path) -> usize {
        planegit::journal(root)
            .iter()
            .filter(|l| l["outcome"] == "committed")
            .count()
    }

    #[test]
    fn a_chat_that_ends_saves_the_plane_at_once_when_auto_save_is_on() {
        let dir = plane("[plane]\nmode = \"commit\"\n");
        let mut state = State::default();
        let at = Instant::now();
        state.look(dir.path(), at, None);
        std::fs::write(dir.path().join("note.md"), "n").unwrap();

        assert!(state.look(
            dir.path(),
            at + Duration::from_secs(1),
            Some(Poke::SessionEnded)
        ));

        let last = &planegit::journal(dir.path())[0];
        assert_eq!(
            (last["trigger"].as_str(), last["outcome"].as_str()),
            (Some("session-end"), Some("committed"))
        );
    }

    #[test]
    fn a_quiet_plane_is_saved_after_its_quiet_period_and_not_before() {
        let dir = plane("[plane]\nmode = \"commit\"\nautosave_after = \"30s\"\n");
        let mut state = State::default();
        let at = Instant::now();
        state.look(dir.path(), at, None);
        std::fs::write(dir.path().join("note.md"), "n").unwrap();

        assert!(!state.look(dir.path(), at + Duration::from_secs(2), None));
        assert!(!state.look(dir.path(), at + Duration::from_secs(31), None));
        assert_eq!(commits(dir.path()), 0);
        assert!(state.look(dir.path(), at + Duration::from_secs(32), None));
        assert_eq!(commits(dir.path()), 1);
    }

    #[test]
    fn a_plane_nobody_has_chosen_a_mode_for_is_never_saved_by_itself() {
        let dir = plane("[memory]\nshare = \"local\"\n");
        let mut state = State::default();
        let at = Instant::now();
        state.look(dir.path(), at, None);
        std::fs::write(dir.path().join("note.md"), "n").unwrap();
        for s in [2, 40, 600] {
            state.look(dir.path(), at + Duration::from_secs(s), None);
        }
        state.look(
            dir.path(),
            at + Duration::from_secs(601),
            Some(Poke::SessionEnded),
        );
        assert!(planegit::journal(dir.path()).is_empty());
    }

    #[test]
    fn a_worker_let_go_of_stops_even_while_a_chats_end_still_holds_its_sender() {
        let dir = plane("[plane]\nmode = \"commit\"\n");
        let worker = Worker::start(
            PlaneId::for_tests(dir.path()),
            dir.path().to_path_buf(),
            Arc::new(|_| {}),
        );
        let held_by_a_chat = worker.poker();

        assert!(
            worker.stop_within(Duration::from_secs(5)),
            "the worker kept looking"
        );
        drop(held_by_a_chat);
    }
}
