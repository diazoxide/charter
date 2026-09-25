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
//!
//! **Workspace repos too, but only those that ask** (charter-app#299): a repo whose
//! `[repos.<name>] autosave` is on is saved the same three ways, after its own quiet period,
//! when a chat ends and at quit. Its save waits while any chat in its workspace is mid-turn
//! ([`MidTurn`]): that cycle is skipped, and its quiet period starts again once the turn ends.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use charter_core::autosave::{self, Decision, Quiet};
use charter_core::planegit::{self, Stage, Trigger};
use charter_core::planesave;
use charter_core::reposave;

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

/// The chats working in a workspace that are mid-turn, by name — [`crate::planes::Held::mid_turn_in`].
pub type MidTurn = Arc<dyn Fn(&str) -> Vec<String> + Send + Sync>;

/// Where the worker asks [`MidTurn`], once the plane it belongs to is built — the same slot
/// shape as the handoff answer's, and for the same reason: the plane holds the worker, so the
/// worker can only hold the plane weakly, and only after it exists.
type MidTurnSlot = Arc<std::sync::Mutex<Option<MidTurn>>>;

/// A plane's auto-save worker. Dropping it stops the loop at its next look: the flag is
/// what stops it, because a chat's end still holds a sender, and a channel alone would keep a
/// let-go plane's worker looking until the last of those was gone.
pub struct Worker {
    poke: Sender<Poke>,
    stop: Arc<AtomicBool>,
    mid_turn: MidTurnSlot,
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

/// Told when auto-save saved the plane at a root, so the extensions that hear a plane being
/// saved are told as they are after the Save button (charter-app#343).
pub type Saved = Arc<dyn Fn(PlaneId, PathBuf) + Send + Sync + 'static>;

impl Worker {
    /// Start looking at the plane at `root`. `changed` tells the window it moved, and `saved`
    /// that it saved it.
    pub fn start(
        plane: PlaneId,
        root: PathBuf,
        changed: crate::planewatch::Changed,
        saved: Saved,
    ) -> Self {
        let (poke, poked) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let mid_turn: MidTurnSlot = Arc::default();
        let asking = Arc::clone(&mid_turn);
        let spawned = std::thread::Builder::new()
            .name("charter-autosave".into())
            .spawn(move || {
                let told = || saved(plane.clone(), root.clone());
                let mid_turn = |workspace: &str| {
                    let asked = asking
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clone();
                    asked.map(|ask| ask(workspace)).unwrap_or_default()
                };
                run(
                    &root,
                    &poked,
                    &stopping,
                    &mid_turn,
                    &|| changed(plane.clone()),
                    &told,
                );
            });
        let thread = spawned
            .map_err(|why| eprintln!("charter: auto-save did not start ({why}); save by hand"))
            .ok();
        Self {
            poke,
            stop,
            mid_turn,
            thread,
        }
    }

    /// Who the worker asks which chats are mid-turn. Until this is said, it says none are —
    /// which is true: no chat of the plane has started before the plane is built.
    pub fn asks_mid_turn_of(&self, ask: MidTurn) {
        *self
            .mid_turn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ask);
    }

    /// Something for the worker to act on at its next look.
    pub fn poker(&self) -> Sender<Poke> {
        self.poke.clone()
    }
}

/// The loop: a look every [`LOOK_EVERY`] or at a poke, until the worker is dropped.
fn run(
    root: &Path,
    poked: &Receiver<Poke>,
    stop: &AtomicBool,
    mid_turn: &dyn Fn(&str) -> Vec<String>,
    tell: &dyn Fn(),
    saved: &dyn Fn(),
) {
    let mut state = State::default();
    let mut poke = None;
    loop {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let now = Instant::now();
        let plane = state.look(root, now, poke);
        let repos = state.look_at_repos(root, now, poke, mid_turn);
        if plane || repos {
            tell();
        }
        if std::mem::take(&mut state.saved) {
            saved();
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
    /// The last look saved the plane, and nobody has been told yet.
    saved: bool,
    /// Each auto-saved repo's quiet period, by its clone's path.
    repos: HashMap<PathBuf, Quiet>,
    repos_launched: bool,
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
            self.saved = crate::saving::save_as(root, None, trigger).is_ok();
            did = true;
        }
        did
    }
}

impl State {
    /// One look at the workspace repos whose `[repos.<name>] autosave` is on — none, unless a
    /// table says so, and then nothing here costs a git process. Answers whether any was saved.
    fn look_at_repos(
        &mut self,
        root: &Path,
        now: Instant,
        poke: Option<Poke>,
        mid_turn: &dyn Fn(&str) -> Vec<String>,
    ) -> bool {
        let launched = std::mem::replace(&mut self.repos_launched, true);
        let settings = planesave::Settings::read(root);
        let saved: Vec<String> = settings
            .repo_tables()
            .into_iter()
            .filter(|name| autosave::repo_on(&settings.repo(name)))
            .collect();
        if saved.is_empty() {
            self.repos.clear();
            return false;
        }
        let Ok(workspaces) = charter_core::workspaces::Plane::open(root).workspaces() else {
            return false;
        };
        let mut did = false;
        for workspace in workspaces {
            let Ok(found) = charter_core::repos::clones(root, &workspace) else {
                continue;
            };
            let mut busy: Option<Vec<String>> = None;
            for repo in found.repos.iter().filter(|r| saved.contains(&r.name)) {
                let quiet = self.repos.entry(repo.path.clone()).or_default();
                let busy = busy.get_or_insert_with(|| mid_turn(&workspace));
                if !busy.is_empty() {
                    // This cycle is skipped, and the quiet period starts again once the turn
                    // is over: the turn was the change.
                    *quiet = Quiet::default();
                    continue;
                }
                let standing = reposave::standing(root, &workspace, repo);
                let settled = standing.stage != Stage::Blocked && standing.worth_saving();
                let trigger = if !launched {
                    (standing.stage == Stage::Committed && standing.pushes)
                        .then_some(Trigger::Launch)
                } else if poke == Some(Poke::SessionEnded) {
                    settled.then_some(Trigger::SessionEnd)
                } else {
                    None
                };
                let trigger = trigger.or_else(|| {
                    (quiet.tick_repo(now, &settings.repo(&repo.name), &standing) == Decision::Save)
                        .then_some(Trigger::Quiet)
                });
                if let Some(trigger) = trigger {
                    // A refusal is in the journal, in its own words; the row says blocked.
                    let _ = reposave::save_as(
                        &reposave::Request {
                            plane: root,
                            workspace: &workspace,
                            name: &repo.name,
                            clone: &repo.path,
                            message: None,
                            no_push: false,
                            mid_turn: busy,
                        },
                        trigger,
                        &mut |_| {},
                    );
                    did = true;
                }
            }
        }
        did
    }
}

/// Every clone whose `[repos.<name>] autosave` is on, in every workspace of the plane at
/// `root`, as `(workspace, clone)`.
fn auto_saved_repos(root: &Path) -> Vec<(String, charter_core::repos::Repo)> {
    let settings = planesave::Settings::read(root);
    let saved: Vec<String> = settings
        .repo_tables()
        .into_iter()
        .filter(|name| autosave::repo_on(&settings.repo(name)))
        .collect();
    if saved.is_empty() {
        return Vec::new();
    }
    let workspaces = charter_core::workspaces::Plane::open(root)
        .workspaces()
        .unwrap_or_default();
    workspaces
        .into_iter()
        .flat_map(|workspace| {
            let found = charter_core::repos::clones(root, &workspace)
                .map(|found| found.repos)
                .unwrap_or_default();
            found
                .into_iter()
                .filter(|repo| saved.contains(&repo.name))
                .map(move |repo| (workspace.clone(), repo))
                .collect::<Vec<_>>()
        })
        .collect()
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
///
/// Each plane's repos are saved beside it — only those whose `[repos.<name>] autosave` is on.
pub fn at_quit(roots: Vec<PathBuf>) {
    let (done, finished) = mpsc::channel();
    let mut started = 0;
    for root in roots {
        for (workspace, repo) in auto_saved_repos(&root) {
            let done = done.clone();
            let plane = root.clone();
            started += 1;
            std::thread::spawn(move || {
                autosave::repo_at_quit(&plane, &workspace, &repo, QUIT_BOUND);
                let _ = done.send(());
            });
        }
        let done = done.clone();
        started += 1;
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

    /// A plane saying `toml`, with a workspace `alpha` holding a clone `widget`.
    fn plane_with_a_repo(toml: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = plane(toml);
        let clone = dir.path().join("workspaces/alpha/widget");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main"]);
        git(&clone, &["config", "user.name", "t"]);
        git(&clone, &["config", "user.email", "t@example.invalid"]);
        std::fs::write(clone.join("README.md"), "one").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "one"]);
        (dir, clone)
    }

    fn repo_commits(root: &Path) -> usize {
        planegit::journal(root)
            .iter()
            .filter(|l| l["target"] == "repo:alpha/widget" && l["outcome"] == "committed")
            .count()
    }

    #[test]
    fn a_repo_with_auto_save_on_is_saved_after_its_quiet_period_but_never_mid_turn() {
        let (dir, clone) = plane_with_a_repo(
            "[repos.widget]\nmode = \"commit\"\nautosave = true\nautosave_after = \"30s\"\n",
        );
        let root = dir.path();
        let mut state = State::default();
        let at = Instant::now();
        let working = |_: &str| vec!["alpha.1".to_owned()];
        let idle = |_: &str| Vec::new();
        state.look_at_repos(root, at, None, &idle);
        std::fs::write(clone.join("a.md"), "a").unwrap();

        // A chat in alpha is mid-turn: every cycle is skipped, however long it runs.
        for s in [1, 31, 90, 300] {
            assert!(!state.look_at_repos(root, at + Duration::from_secs(s), None, &working));
        }
        assert!(
            !state.look_at_repos(
                root,
                at + Duration::from_secs(301),
                Some(Poke::SessionEnded),
                &working
            ),
            "a chat ending elsewhere while one here still works is not a reason"
        );
        assert_eq!(repo_commits(root), 0);

        // The turn ends; the quiet period starts then.
        assert!(!state.look_at_repos(root, at + Duration::from_secs(302), None, &idle));
        assert!(!state.look_at_repos(root, at + Duration::from_secs(331), None, &idle));
        assert!(state.look_at_repos(root, at + Duration::from_secs(332), None, &idle));
        assert_eq!(repo_commits(root), 1);
    }

    #[test]
    fn a_repo_whose_table_leaves_auto_save_off_is_never_saved_by_itself() {
        let (dir, clone) = plane_with_a_repo("[repos.widget]\nmode = \"commit\"\n");
        let root = dir.path();
        let mut state = State::default();
        let at = Instant::now();
        let idle = |_: &str| Vec::new();
        std::fs::write(clone.join("a.md"), "a").unwrap();
        for s in [0, 31, 62, 400] {
            state.look_at_repos(root, at + Duration::from_secs(s), None, &idle);
        }
        state.look_at_repos(
            root,
            at + Duration::from_secs(401),
            Some(Poke::SessionEnded),
            &idle,
        );
        assert_eq!(repo_commits(root), 0);
    }

    #[test]
    fn a_worker_let_go_of_stops_even_while_a_chats_end_still_holds_its_sender() {
        let dir = plane("[plane]\nmode = \"commit\"\n");
        let worker = Worker::start(
            PlaneId::for_tests(dir.path()),
            dir.path().to_path_buf(),
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        );
        let held_by_a_chat = worker.poker();

        assert!(
            worker.stop_within(Duration::from_secs(5)),
            "the worker kept looking"
        );
        drop(held_by_a_chat);
    }
}
